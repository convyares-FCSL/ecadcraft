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
//! This module draws nothing and records nothing; `ecad_cadcraft::cmd::style` builds the ops.

use std::collections::BTreeSet;

use ecad_model::style::{
    self as ms, MAX_FIELD_CHARS, MAX_FIELDS_PER_STYLE, MAX_STYLES_PER_CATALOGUE, StyleCatalogue, StyleLayer, StyleRecord, StyleRef,
};
use ecad_model::{Document, ObjectRef, Op, PageId, Properties, Subject, Word};

use crate::CmdError;
use crate::defaults::{self, Kind, Resolved, Scope, Slot, Source, Value};

pub mod cable;
pub mod checks;
pub mod frame;
pub mod location;
pub mod page;
pub mod ratingtable;
pub mod text;
pub mod wire;

/// One field of a catalogue.
#[derive(Debug)]
pub struct Field {
    /// The key in style records: `height`, `line_weight`.
    pub key: &'static str,
    /// Words for tables and messages: `Cap height`.
    pub label: &'static str,
    pub kind: Kind,
    /// The value when no level sets one (step 5 of design 050 §2.4), read from the data files'
    /// `[fields.<catalogue>]` on first use ([`Field::builtin`], brief 066).
    pub slot: Slot<Value<'static>>,
    /// The object property that overrides it (050b: a conductor's own `colour`), if any.
    pub over: Option<&'static str>,
}

/// A built-in style: a word ID, never renamed or reused, a name, and the values it sets.
#[derive(Debug)]
pub struct Builtin {
    pub id: &'static str,
    pub name: &'static str,
    /// The field values it sets, read from the data files' `[styles.<catalogue>.<id>]` on first
    /// use ([`Builtin::values`], brief 066).
    pub slot: Slot<Vec<(&'static str, Value<'static>)>>,
}

impl Builtin {
    /// The field values it sets, from the data, in its catalogue's field order.
    pub fn values(&self) -> &[(&'static str, Value<'static>)] {
        self.slot.get_or_init(|| defaults::data::style_values(self))
    }

    /// Its value of field `key`, if it sets one.
    pub fn value(&self, key: &str) -> Option<Value<'static>> {
        self.values().iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }
}

impl Field {
    /// The field's built-in value, from the data (brief 066): read once, then a field read. A
    /// missing or damaged one gives the kind's safe value, never a crash.
    pub fn builtin(&self) -> Value<'static> {
        *self.slot.get_or_init(|| defaults::data::field_builtin(self))
    }
}

/// A project-level value read from somewhere other than the style record (the wire label height
/// in `settings.wiring`, design 050 §4.1): `(doc, style, field)` → the value, or `None`.
pub type ProjectField = for<'a> fn(&'a Document, &StyleRef, &str) -> Option<Value<'a>>;

/// One kind of style.
#[derive(Debug)]
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
    &wire::CATALOGUE,
    &page::CATALOGUE,
    &cable::CATALOGUE,
    &ratingtable::CATALOGUE,
];

/// The catalogue's description.
pub fn catalogue(c: StyleCatalogue) -> &'static Catalogue {
    match c {
        StyleCatalogue::Text => &text::CATALOGUE,
        StyleCatalogue::Location => &location::CATALOGUE,
        StyleCatalogue::Frame => &frame::CATALOGUE,
        StyleCatalogue::Wire => &wire::CATALOGUE,
        StyleCatalogue::Page => &page::CATALOGUE,
        StyleCatalogue::Cable => &cable::CATALOGUE,
        StyleCatalogue::RatingTable => &ratingtable::CATALOGUE,
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
    // Past the bound only: the rank costs at most the bound.
    if map.len() > MAX_STYLES_PER_CATALOGUE && map.range(..id).take(MAX_STYLES_PER_CATALOGUE).count() >= MAX_STYLES_PER_CATALOGUE {
        return None;
    }
    Some(r)
}

/// The longest value field `key` of `catalogue` may hold: [`MAX_FIELD_CHARS`], except a rating
/// table's rows (brief 057, FP8), which hold a CSV data set of at most
/// [`crate::rating::dataset::MAX_BYTES`]. Guard: it keeps every stored value bounded.
pub fn field_cap(catalogue: StyleCatalogue, key: &str) -> usize {
    match (catalogue, key) {
        (StyleCatalogue::RatingTable, ratingtable::ROWS) => crate::rating::dataset::MAX_BYTES,
        _ => MAX_FIELD_CHARS,
    }
}

/// A record's stored value of `key`, within the bounds (at most [`MAX_FIELDS_PER_STYLE`] fields,
/// values of at most [`field_cap`] characters).
fn stored<'a>(r: &'a StyleRecord, catalogue: StyleCatalogue, key: &str) -> Option<&'a str> {
    let v = r.fields.get(key)?;
    if r.fields.len() > MAX_FIELDS_PER_STYLE
        && r.fields.keys().take_while(|k| k.as_str() < key).take(MAX_FIELDS_PER_STYLE).count() >= MAX_FIELDS_PER_STYLE
    {
        return None;
    }
    (v.chars().nth(field_cap(catalogue, key)).is_none()).then_some(v.as_str())
}

/// Whether `id` names a style of `catalogue` in `doc`: a built-in one, or a record in the project
/// or the standard (within the bound).
pub fn exists(doc: &Document, catalogue: StyleCatalogue, id: &StyleRef) -> bool {
    self::catalogue(catalogue).builtin(id).is_some()
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
    if let Some(value) = record(doc, StyleLayer::Project, cat.catalogue, id).and_then(|r| stored(r, cat.catalogue, f.key)).and_then(parse) {
        return Resolved { value, from: Source::Project };
    }
    if let Some(value) = record(doc, StyleLayer::Standard, cat.catalogue, id).and_then(|r| stored(r, cat.catalogue, f.key)).and_then(parse) {
        return Resolved { value, from: Source::Standard };
    }
    if let Some(value) = cat.builtin(id).and_then(|b| b.value(f.key)) {
        return Resolved { value, from: Source::Builtin };
    }
    Resolved { value: f.builtin(), from: Source::Builtin }
}

/// [`field`] by catalogue word and field key; the field's built-in for an unknown one.
pub fn field_of<'a>(doc: &'a Document, catalogue: StyleCatalogue, id: &StyleRef, key: &str, over: Option<&'a Properties>) -> Option<Resolved<'a>> {
    let c = self::catalogue(catalogue);
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

/// The name of style `id` in `catalogue`: the project record's, the standard's, the built-in's,
/// else its ID.
pub fn name(doc: &Document, catalogue: StyleCatalogue, id: &StyleRef) -> String {
    let named = |l| record(doc, l, catalogue, id).map(|r| r.name.as_str()).filter(|n| !n.trim().is_empty());
    if let Some(n) = named(StyleLayer::Project).or_else(|| named(StyleLayer::Standard)) {
        return shown_name(n);
    }
    match self::catalogue(catalogue).builtin(id) {
        Some(b) => b.name.to_string(),
        None => id.text(),
    }
}

/// The styles of `cat` (design 050 §7.2): built-in first (in the catalogue's order), then the
/// standard's own, then the project's own, each group by name. An own style the standard holds is
/// listed once, as the standard's, also when the project overrides fields of it.
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
            .map(|(id, _)| Row { name: name(doc, cat.catalogue, id), id: id.clone(), layer, overridden: false })
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
pub fn find(doc: &Document, catalogue: StyleCatalogue, text: &str) -> Result<StyleRef, CmdError> {
    let t = text.trim();
    if let Some(id) = StyleRef::parse(t).filter(|id| exists(doc, catalogue, id)) {
        return Ok(id);
    }
    let cat = self::catalogue(catalogue);
    let hits: Vec<Row> = rows(doc, cat).into_iter().filter(|r| r.name.eq_ignore_ascii_case(t)).collect();
    match hits.as_slice() {
        [one] => Ok(one.id.clone()),
        [] => Err(CmdError::refused(format!("No {} is called {t:?}.", cat.one.to_lowercase()))),
        _ => Err(CmdError::refused(format!("{} {} are called {t:?}; give the ID ({}).", hits.len(), cat.plural.to_lowercase(), ids(&hits)))),
    }
}

fn ids(rows: &[Row]) -> String {
    rows.iter().map(|r| r.id.text()).collect::<Vec<_>>().join(", ")
}

/// The catalogue words, for messages.
pub fn words() -> String {
    CATALOGUES.iter().map(|c| c.catalogue.word()).collect::<Vec<_>>().join(", ")
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
pub fn users(doc: &Document, catalogue: StyleCatalogue, id: &StyleRef) -> Count {
    let cat = self::catalogue(catalogue);
    let value = id.text();
    let mut c = Count::default();
    // Brief 057: a cable's `cable_type` is a plain cable property (cables have no registry level).
    if cat.catalogue == StyleCatalogue::Cable {
        c.explicit = cable::users(doc, id).len();
    }
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
    match cat.catalogue {
        ecad_model::StyleCatalogue::Frame => doc.pages.iter().filter(|(_, p)| !own(&p.display)).filter(|_| project == *id).count(),
        // Brief 054: pages with no own type whose generated kind (or none) gives `id`.
        ecad_model::StyleCatalogue::Page => doc
            .pages
            .values()
            .filter(|p| !p.page_type.as_ref().is_some_and(|t| crate::styles::exists(doc, ecad_model::StyleCatalogue::Page, t)))
            .filter(|p| ecad_model::sheet::page_type_of(doc, p) == *id)
            .count(),
        ecad_model::StyleCatalogue::Location if project == *id => {
            doc.devices.iter().filter(|(c, x)| crate::locations::is_box(doc, **c) && !own(&x.properties)).count()
        }
        // 050b: conductors whose resolved category is `id` and that have none of their own.
        ecad_model::StyleCatalogue::Wire => {
            let looks = wire::WireLooks::new(doc);
            doc.wires.iter().filter(|(_, x)| !own(&x.display)).filter(|(c, _)| looks.category(doc, **c).0.as_ref() == Some(id)).count()
        }
        ecad_model::StyleCatalogue::Text if project == *id => match text::noun(entry.key) {
            text::Noun::Devices => doc.devices.values().filter(|x| !own(&x.properties)).count(),
            text::Noun::Wires => doc.wires.values().filter(|x| !own(&x.display)).count(),
            text::Noun::Pages => doc.pages.len(),
        },
        _ => 0,
    }
}

/// The explicit users of `id` (any choice key of `catalogue`), through the index.
pub fn explicit_users(doc: &Document, catalogue: StyleCatalogue, id: &StyleRef) -> Vec<(&'static str, ObjectRef)> {
    let cat = self::catalogue(catalogue);
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
        ObjectRef::Device(c) => out.extend(doc.representations_of(Subject::Device(c)).map(|(_, r)| r.page)),
        ObjectRef::Wire(c) => out.extend(crate::wires::wire_pages(doc, c)),
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
            let cat = self::catalogue(*catalogue);
            // Brief 054 (design 054 §10.2): a page type reaches its pages only.
            if let Some((pages, _)) = page::touched_by(doc, op) {
                return Some(pages.into_iter().collect());
            }
            // A style the project (or standard, or built-in) choice resolves to is on every page.
            if (cat.choices)().iter().any(|e| chosen(doc, e, Scope::project()) == *id) {
                return None;
            }
            // A text style a location style labels with reaches every box.
            if cat.catalogue == ecad_model::StyleCatalogue::Text && location::labels_with(doc, id) {
                return None;
            }
            // 050b: a category that is (or becomes, or was) a potential's default reaches every
            // net of that potential, on any page (there is no potential index): every page.
            if cat.catalogue == ecad_model::StyleCatalogue::Wire {
                let names_potential = |r: &Option<StyleRecord>| r.as_ref().is_some_and(|r| r.fields.contains_key("potential"));
                let (before, after) = match op {
                    Op::SetStyle { before, after, .. } => (before, after),
                    _ => return None,
                };
                let looks = wire::WireLooks::new(doc);
                let default = looks.by_potential_ids().any(|x| x == id);
                if default || names_potential(before) || names_potential(after) {
                    return None;
                }
            }
            let mut out = BTreeSet::new();
            for (_, o) in explicit_users(doc, *catalogue, id) {
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
pub fn check_record(doc: &Document, cat: &Catalogue, id: &StyleRef, layer: StyleLayer, r: &StyleRecord) -> Result<(), CmdError> {
    if !r.name.is_empty() && !ms::is_name(&r.name) {
        return Err(CmdError::refused("A style needs a name (1 to 64 characters, no control characters)."));
    }
    if r.fields.len() > MAX_FIELDS_PER_STYLE {
        return Err(CmdError::refused(format!("A style holds at most {MAX_FIELDS_PER_STYLE} fields.")));
    }
    if let Some((k, _)) = r.fields.iter().find(|(k, v)| v.chars().nth(field_cap(cat.catalogue, k)).is_some()) {
        return Err(CmdError::refused(format!("{k} is at most {} characters.", field_cap(cat.catalogue, k))));
    }
    // Brief 057 (design 057 §3.1): a rating's reference temperature is below the conductor's
    // maximum.
    if let Some(why) = crate::rating::check_style(cat.catalogue, r) {
        return Err(CmdError::refused(why));
    }
    // 050b (design 050 §5.4): one category per potential.
    if cat.catalogue == ecad_model::StyleCatalogue::Wire
        && let Some(p) = r.fields.get("potential").map(|p| p.trim()).filter(|p| !p.is_empty())
        && let Some(other) = wire::WireLooks::new(doc).for_potential(p).filter(|o| *o != id)
    {
        return Err(CmdError::refused(format!(
            "{} is already the default for {p}; clear it there first.",
            name(doc, ecad_model::StyleCatalogue::Wire, other)
        )));
    }
    let held = ms::layer_of(doc, layer).and_then(|c| c.get(&cat.catalogue));
    if held.is_some_and(|m| !m.contains_key(id) && m.len() >= MAX_STYLES_PER_CATALOGUE) {
        return Err(CmdError::refused(format!("{} holds at most {MAX_STYLES_PER_CATALOGUE} styles per layer.", cat.plural)));
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
    t.eq_ignore_ascii_case("none") || hex_rgb(t).is_some() || crate::words::colour_code(t).is_some_and(|(_, two)| two.is_none())
}

pub fn hex_rgb(t: &str) -> Option<(u8, u8, u8)> {
    let h = t.strip_prefix('#')?;
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some(((n >> 16) as u8, (n >> 8) as u8, n as u8))
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
            assert!(ms::is_word(c.catalogue.word()), "{}", c.catalogue);
            for f in c.fields {
                // 050b (design 050 §5.3): an empty built-in is "no value" (potential, part number).
                let empty = matches!(f.builtin(), Value::Text(""));
                assert!(empty || defaults::accepts(f.kind, &f.builtin()), "{}.{} built-in", c.catalogue, f.key);
            }
            for b in c.builtins {
                assert!(ms::is_word(b.id), "{}", b.id);
                for (k, v) in b.values() {
                    let f = c.field(k);
                    assert!(f.is_some(), "{}.{} names no field {k}", c.catalogue, b.id);
                    assert!(f.is_some_and(|f| defaults::accepts(f.kind, v)), "{}.{}.{k}", c.catalogue, b.id);
                }
            }
            for e in (c.choices)() {
                assert!(matches!(e.kind, Kind::Style(w) if w == c.catalogue), "{}", e.key);
                // 050b: the category choice's built-in is `none`, no category.
                let b = StyleRef::parse(&builtin_word(e));
                // 057: no cable type and no rating table, either.
                let none = matches!(
                    c.catalogue,
                    ecad_model::StyleCatalogue::Wire | ecad_model::StyleCatalogue::Cable | ecad_model::StyleCatalogue::RatingTable
                ) && builtin_word(e) == wire::NONE;
                assert!(none || b.as_ref().is_some_and(|b| c.builtin(b).is_some()), "{}: built-in names no built-in style", e.key);
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
