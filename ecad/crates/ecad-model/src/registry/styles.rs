//! Style catalogues: named styles that objects refer to by a stable ID, so changing a style changes
//! every object that uses it.
//!
//! # Semantics
//! - A [`Catalogue`] declares its [`Field`]s and ships built-in styles ([`Builtin`]). Standards
//!   and projects add **own** styles and override fields of built-in ones with style records
//!   ([`crate::objects::styles`]).
//! - **Which style** an object uses is a registry choice entry ([`Kind::Style`]), resolved like
//!   any entry. **What a style says** is [`field`]: the object's override (for a field that names
//!   one), the project's record, the standard's record, the built-in style, then the field's
//!   built-in. A stored value that does not parse counts as absent, so a hostile file never gives a
//!   value outside the field's range.
//! - A reference to a style that does not exist (a hostile or merged file) counts as absent and
//!   falls through to the next level; Checks reports it.
//!
//! The catalogues themselves are declared by the crates that know them.

use std::fmt;

use crate::objects::styles::{self as ms, MAX_STYLES_PER_CATALOGUE, StyleBody, StyleCatalogue, StyleLayer, StyleRecord, StyleRef};
use crate::registry::data::Slot;
use crate::registry::{Entry, Kind, Resolved, Scope, Source, Value, accepts, parse_value_in, resolve};
use crate::{Document, Properties};

/// One field of a catalogue.
#[derive(Debug)]
pub struct Field {
    /// The key in style records: `height`, `line_weight`.
    pub key: &'static str,
    /// Words for tables and messages: `Cap height`.
    pub label: &'static str,
    pub kind: Kind,
    /// The value when no level sets one, read from the data files' `[fields.<catalogue>]` on first
    /// use ([`Field::builtin`]).
    pub slot: Slot<Value<'static>>,
    /// The object property that overrides it (a conductor's own `colour`), if any.
    pub over: Option<&'static str>,
}

/// A built-in style: a word ID, never renamed or reused, a name, and the values it sets.
#[derive(Debug)]
pub struct Builtin {
    pub id: &'static str,
    pub name: &'static str,
    /// The field values it sets, read from the data files' `[styles.<catalogue>.<id>]` on first use
    /// ([`Builtin::values`]).
    pub slot: Slot<Vec<(&'static str, Value<'static>)>>,
}

impl Builtin {
    /// The field values it sets, from the data, in its catalogue's field order.
    pub fn values(&self) -> &[(&'static str, Value<'static>)] {
        self.slot.get_or_init(|| self.slot.data().style_values(self))
    }

    /// Its value of field `key`, if it sets one.
    pub fn value(&self, key: &str) -> Option<Value<'static>> {
        self.values().iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }
}

impl Field {
    /// The field's built-in value, from the data: read once, then a field read. A missing or
    /// damaged one gives the kind's safe value, never a crash.
    pub fn builtin(&self) -> Value<'static> {
        *self.slot.get_or_init(|| self.slot.data().field_builtin(self))
    }
}

/// A project-level value read from somewhere other than the style record (the wire label height in
/// `settings.wiring`): `(doc, style, field)` → the value, or `None`.
pub type ProjectField = for<'a> fn(&'a Document, &StyleRef, &str) -> Option<Value<'a>>;

/// The value field `key` of a typed record body holds, before the field's range check.
pub(crate) type BodyField = for<'a> fn(&'a Document, &'a StyleBody, &str) -> Option<Value<'a>>;

/// A catalogue whose built-ins are typed records rather than data-file tables.
#[derive(Clone, Copy)]
pub struct Typed {
    /// A field's built-in value.
    pub field: fn(&Field) -> Value<'static>,
    /// A built-in style's field values.
    pub values: fn(&Builtin) -> Vec<(&'static str, Value<'static>)>,
}

/// One kind of style.
pub struct Catalogue {
    /// Which catalogue it is (its stored word: `text`, `location`, `frame`, …).
    pub catalogue: StyleCatalogue,
    /// Its table's name: `Text styles`.
    pub plural: &'static str,
    /// One of its styles in words: `Text style`.
    pub one: &'static str,
    pub fields: &'static [Field],
    /// Built-in styles, in table order.
    pub builtins: &'static [Builtin],
    /// The registry's choice entries naming this catalogue's styles.
    pub choices: fn() -> Vec<&'static Entry>,
    pub project_field: Option<ProjectField>,
    /// The value a record's typed body holds for a field.
    pub body: BodyField,
    /// Built-ins read from typed records instead of the data files.
    pub typed: Option<Typed>,
    /// A change to a built-in style's values read from the data files.
    pub adjust: Option<fn(&Builtin, &mut Vec<(&'static str, Value<'static>)>)>,
    /// Write field `key` of a record body of this catalogue (`None` clears it). A value the
    /// field does not take is refused in words.
    pub set: fn(&mut StyleBody, &str, Option<&str>) -> Result<(), Refused>,
    /// How many objects with no choice of their own use style `id` through `entry`'s page,
    /// project, standard or built-in level.
    pub implicit: fn(&Document, &Entry, &StyleRef) -> usize,
    /// The pages a change `op` to style `id` reaches besides those its explicit users are on.
    pub reach: fn(&Document, &crate::Op, &StyleRef) -> Reach,
}

/// Whether `props` hold a choice of `entry` of their own that reads.
pub fn chooses(doc: &Document, entry: &Entry, props: &Properties) -> bool {
    props.get(entry.key).is_some_and(|v| entry.parse_in(doc, v).is_some())
}

/// A value a catalogue's field does not take ([`Catalogue::set`]), in words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Refused(pub String);

/// The pages a style change reaches ([`Catalogue::reach`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// These pages only, whoever uses the style.
    Only(std::collections::BTreeSet<crate::PageId>),
    /// These pages as well as its explicit users' pages.
    Also(std::collections::BTreeSet<crate::PageId>),
    /// Every page.
    Every,
}

/// A catalogue whose objects all choose their style themselves: no implicit users.
pub fn no_implicit(_: &Document, _: &Entry, _: &StyleRef) -> usize {
    0
}

/// A catalogue whose style changes reach only their explicit users' pages.
pub fn no_reach(_: &Document, _: &crate::Op, _: &StyleRef) -> Reach {
    Reach::Also(std::collections::BTreeSet::new())
}

impl fmt::Debug for Catalogue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Catalogue({})", self.catalogue)
    }
}

/// Two catalogues are the same when they describe the same word.
impl PartialEq for Catalogue {
    fn eq(&self, other: &Self) -> bool {
        self.catalogue == other.catalogue
    }
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

/// The records `layer` holds for `catalogue` in `doc`, at most [`MAX_STYLES_PER_CATALOGUE`] in ID
/// order (the bound's excess is ignored here and reported by Checks).
pub fn records<'a>(doc: &'a Document, layer: StyleLayer, catalogue: StyleCatalogue) -> impl Iterator<Item = (&'a StyleRef, &'a StyleRecord)> + 'a {
    ms::layer_of(doc, layer).and_then(move |c| c.get(&catalogue)).into_iter().flat_map(|m| m.iter()).take(MAX_STYLES_PER_CATALOGUE)
}

/// The record `layer` holds for `(catalogue, id)`, within the bound.
pub fn record<'a>(doc: &'a Document, layer: StyleLayer, catalogue: StyleCatalogue, id: &StyleRef) -> Option<&'a StyleRecord> {
    let map = ms::layer_of(doc, layer)?.get(&catalogue)?;
    let r = map.get(id)?;
    if map.len() > MAX_STYLES_PER_CATALOGUE && map.range(..id).take(MAX_STYLES_PER_CATALOGUE).count() >= MAX_STYLES_PER_CATALOGUE {
        return None;
    }
    Some(r)
}

/// Whether `v` is in field kind `kind`'s range (a typed body's numbers are not checked on read).
fn in_range(kind: Kind, v: &Value<'_>) -> bool {
    match (kind, v) {
        (Kind::Length { min, max } | Kind::Number { min, max }, Value::Length(x) | Value::Number(x)) => x.is_finite() && (min..=max).contains(x),
        _ => true,
    }
}

/// The text of the value record `r` sets for field `key` (tables of what each layer stores).
pub fn record_text(r: &StyleRecord, key: &str) -> Option<String> {
    r.body.entry(key)
}

/// The value field `f` of record `r` of catalogue `cat` sets: its typed body's field, within the
/// field's range.
fn record_value<'a>(doc: &'a Document, cat: &Catalogue, r: &'a StyleRecord, f: &Field) -> Option<Value<'a>> {
    (cat.body)(doc, &r.body, f.key).filter(|v| in_range(f.kind, v))
}

/// Whether `id` names a style of `catalogue` in `doc`: a built-in one, or a record in the project
/// or the standard (within the bound).
pub fn exists(doc: &Document, catalogue: &Catalogue, id: &StyleRef) -> bool {
    catalogue.builtin(id).is_some()
        || record(doc, StyleLayer::Project, catalogue.catalogue, id).is_some()
        || record(doc, StyleLayer::Standard, catalogue.catalogue, id).is_some()
}

/// The value of field `f` of style `id`: `over`'s property when the field names one, the project's
/// record, the standard's, the built-in style's, the field's built-in. Never fails; allocates
/// nothing.
pub fn field<'a>(doc: &'a Document, cat: &Catalogue, id: &StyleRef, f: &Field, over: Option<&'a Properties>) -> Resolved<'a> {
    let parse = |s: &'a str| parse_value_in(doc, f.kind, s);
    if let Some(value) = f.over.zip(over).and_then(|(k, p)| p.get(k)).and_then(|v| parse(v.as_str())) {
        return Resolved { value, from: Source::Object };
    }
    if let Some(value) = cat.project_field.and_then(|read| read(doc, id, f.key)).filter(|v| accepts(f.kind, v)) {
        return Resolved { value, from: Source::Project };
    }
    if let Some(value) = record(doc, StyleLayer::Project, cat.catalogue, id).and_then(|r| record_value(doc, cat, r, f)) {
        return Resolved { value, from: Source::Project };
    }
    if let Some(value) = record(doc, StyleLayer::Standard, cat.catalogue, id).and_then(|r| record_value(doc, cat, r, f)) {
        return Resolved { value, from: Source::Standard };
    }
    if let Some(value) = cat.builtin(id).and_then(|b| b.value(f.key)) {
        return Resolved { value, from: Source::Builtin };
    }
    Resolved { value: f.builtin(), from: Source::Builtin }
}

/// [`field`] by field key; `None` for a key the catalogue does not have.
pub fn field_of<'a>(doc: &'a Document, cat: &Catalogue, id: &StyleRef, key: &str, over: Option<&'a Properties>) -> Option<Resolved<'a>> {
    let f = cat.field(key)?;
    Some(field(doc, cat, id, f, over))
}

/// The style a choice entry resolves to at `at`: its stored ID when it names a style, else the
/// entry's built-in.
pub fn chosen(doc: &Document, entry: &Entry, at: Scope) -> StyleRef {
    let r = resolve(doc, entry, at);
    StyleRef::parse(&r.text()).unwrap_or_else(|| StyleRef::Builtin(builtin_word(entry)))
}

/// The word of a choice entry's built-in style.
pub fn builtin_word(entry: &Entry) -> String {
    match entry.builtin() {
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

/// The name of style `id` in `cat`: the project record's, the standard's, the built-in's, else its
/// ID.
pub fn name(doc: &Document, cat: &Catalogue, id: &StyleRef) -> String {
    let named = |l| record(doc, l, cat.catalogue, id).and_then(|r| r.name.as_ref()).map(|n| n.as_str());
    if let Some(n) = named(StyleLayer::Project).or_else(|| named(StyleLayer::Standard)) {
        return shown_name(n);
    }
    match cat.builtin(id) {
        Some(b) => b.name.to_string(),
        None => id.text(),
    }
}

/// The styles of `cat`: built-in first (in the catalogue's order), then the standard's own, then
/// the project's own, each group by name. An own style the standard holds is listed once, as the
/// standard's, also when the project overrides fields of it.
pub fn rows(doc: &Document, cat: &Catalogue) -> Vec<Row> {
    let has = |l, id: &StyleRef| record(doc, l, cat.catalogue, id).is_some();
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
        let mut v: Vec<Row> = records(doc, l, cat.catalogue)
            .filter(|(id, _)| cat.builtin(id).is_none() && !skip(id))
            .map(|(id, _)| Row { name: name(doc, cat, id), id: id.clone(), layer, overridden: false })
            .collect();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
        v
    };
    out.extend(own(StyleLayer::Standard, Layer::Standard, &|_| false));
    out.extend(own(StyleLayer::Project, Layer::Project, &|id| has(StyleLayer::Standard, id)));
    out
}

/// The row [`rows`] lists for style `id` of `cat`, looked up alone; `None` when `cat` has no such style.
pub fn row(doc: &Document, cat: &Catalogue, id: &StyleRef) -> Option<Row> {
    let has = |l| record(doc, l, cat.catalogue, id).is_some();
    if let Some(b) = cat.builtin(id) {
        let overridden = has(StyleLayer::Project) || has(StyleLayer::Standard);
        return Some(Row { id: id.clone(), name: b.name.to_string(), layer: Layer::Builtin, overridden });
    }
    let layer = if has(StyleLayer::Standard) {
        Layer::Standard
    } else if has(StyleLayer::Project) {
        Layer::Project
    } else {
        return None;
    };
    Some(Row { id: id.clone(), name: name(doc, cat, id), layer, overridden: false })
}

/// The styles of `cat` whose [`rows`] name is `text`, ignoring case, in the order [`rows`] lists them: one
/// pass over the names, without building or sorting the rows.
pub fn called(doc: &Document, cat: &Catalogue, text: &str) -> Vec<StyleRef> {
    let builtins = cat.builtins.iter().filter(|b| b.name.eq_ignore_ascii_case(text)).map(|b| StyleRef::Builtin(b.id.to_string()));
    let own = ids(doc, cat).into_iter().filter(|id| cat.builtin(id).is_none() && name(doc, cat, id).eq_ignore_ascii_case(text));
    builtins.chain(own).collect()
}

/// The IDs of [`rows`], without their names or order: for a reader that looks every style up by
/// ID, so reading them costs one pass over the records.
pub fn ids(doc: &Document, cat: &Catalogue) -> Vec<StyleRef> {
    let mut out: Vec<StyleRef> = cat.builtins.iter().map(|b| StyleRef::Builtin(b.id.to_string())).collect();
    out.extend(records(doc, StyleLayer::Standard, cat.catalogue).filter(|(id, _)| cat.builtin(id).is_none()).map(|(id, _)| id.clone()));
    out.extend(
        records(doc, StyleLayer::Project, cat.catalogue)
            .filter(|(id, _)| cat.builtin(id).is_none() && record(doc, StyleLayer::Standard, cat.catalogue, id).is_none())
            .map(|(id, _)| id.clone()),
    );
    out
}

/// How many objects use a style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Count {
    /// Objects whose own choice names it.
    pub explicit: usize,
    /// Objects with no own choice whose page, project, standard or built-in choice resolves to it.
    pub implicit: usize,
}

/// `#rrggbb` as its three bytes; `None` for anything else.
pub fn hex_rgb(t: &str) -> Option<(u8, u8, u8)> {
    let h = t.strip_prefix('#')?;
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some(((n >> 16) as u8, (n >> 8) as u8, n as u8))
}
