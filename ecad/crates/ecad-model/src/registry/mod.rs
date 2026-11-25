//! The defaults registry: its entries, how a value is resolved, and the built-in data.
//!
//! Every default (a size, a weight, a show rule, a gap…) is **one entry** ([`Entry`]), declared by
//! the crate that knows it, with its built-in values in that crate's [`data::DataSet`]. Code that
//! needs the value asks [`resolve`] (or a typed reader over it); it never writes the number where
//! it is used. Style catalogues ([`styles::Catalogue`]) are declared the same way.
//!
//! Not entries: hostile-input bounds (most candidates, most texts per page…). They are guards, not
//! preferences, and stay constants beside the code they guard.
//!
//! # Resolution
//!
//! Narrowest level first, only the levels the entry allows: object (a symbol's
//! `text.<rep>.<rest>` key on its component, then the component's or conductor's own `key`) →
//! page → project (the project property `key`, or [`ProjectSource::Settings`]) → standard (the
//! project's standard's values) → the built-in. A stored value that does not parse or
//! is out of range counts as absent, so a hostile file never gives a value outside the entry's
//! range. A style choice ([`Kind::Style`]) also counts as absent when no style of that ID exists
//! in its catalogue.

pub mod data;
pub mod styles;

use crate::registry::data::Slot;
use crate::registry::styles::Catalogue;
use crate::{DeviceId, Document, LengthUnit, PageId, ProjectSettings, RepresentationId, Subject, WireId};

/// The areas a key may start with: the words of [`crate::naming::keys::Area::REGISTRY`], in its
/// order.
pub const AREAS: &[&str] = <crate::naming::keys::Area as crate::Word>::WORDS.split_at(crate::naming::keys::Area::REGISTRY.len()).0;

/// One registered default.
#[derive(Debug)]
pub struct Entry {
    /// Stable key, also the stored property key: `"text.tag.height"`. Lower case, dot-separated,
    /// first part the area (`text`, `wire`, `print`, `symbol`). Never renamed once released.
    pub key: &'static str,
    /// Words for the Standards screen and `defaults`: `"Tag height"`.
    pub label: &'static str,
    /// Group heading on the Standards screen: `"Text"`, `"Wire labels"`…
    pub group: &'static str,
    pub kind: Kind,
    /// Its built-in value, read from the data files on first use ([`Entry::builtin`]):
    /// the value itself is never written in Rust.
    pub slot: Slot<Value<'static>>,
    /// The levels that may override it.
    pub levels: &'static [Level],
    /// Where the project level is stored, when it is not the project property `key`.
    pub project: ProjectSource,
}

/// What an entry holds, and the check a stored value must pass.
#[derive(Debug, Clone, Copy)]
pub enum Kind {
    /// A length in millimetres (divided by 25.4 in an inch project, see [`Resolved::length`]), in
    /// this range.
    Length { min: f64, max: f64 },
    /// A plain number (a factor), in this range.
    Number { min: f64, max: f64 },
    /// One of these words (compared without case; the registered spelling is returned).
    Choice(&'static [&'static str]),
    /// A switch: stored as exactly `yes` or `no`; typed as any word
    /// [`crate::naming::words::flag`] reads.
    Flag,
    /// Free text of 1 to this many characters, no control characters.
    Text { max_chars: usize },
    /// Text of 1 to `max_chars` characters, no control characters, that `parse` accepts (a
    /// declared parser, e.g. a tag prefix or a revision pattern).
    Parsed { max_chars: usize, parse: fn(&str) -> bool },
    /// A style of this catalogue: its ID, a built-in word or
    /// `#<32 hex>`. A stored ID naming no style of the catalogue counts as absent.
    Style(&'static Catalogue),
}

/// A value of an entry: its built-in, or a stored value that parsed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value<'a> {
    /// Millimetres.
    Length(f64),
    Number(f64),
    /// One of a [`Kind::Choice`] entry's words.
    Choice(&'static str),
    /// A [`Kind::Flag`] entry's switch.
    Flag(bool),
    /// A [`Kind::Text`] or [`Kind::Parsed`] value, or a [`Kind::Style`] value naming a built-in
    /// style by its word.
    Text(&'a str),
    /// A [`Kind::Style`] value naming an own style (a standard's or the project's) by its ID:
    /// held as the ID, so a value read from a typed field needs no stored text.
    OwnStyle(u128),
    /// A typed style field's wire size: a wire type's `size`.
    Size(crate::objects::wire::WireSize),
    /// A typed style field's ordered colours: a wire type's `colours`.
    Colours(&'a crate::objects::wire::Colours),
    /// A typed style field's colour: a text, location or frame style's.
    Colour(crate::objects::styles::StyleColour),
    /// A typed style field's sheet: a page type's or a frame style's own sheet.
    Sheet(crate::document::sheet::SheetSpec),
    /// A typed style field's margins: a frame style's.
    Margins(crate::document::sheet::Margins),
    /// A typed style field's core colours: a cable type's.
    CoreColours(&'a crate::objects::styles::cable::CoreColours),
}

impl Value<'_> {
    /// The style a [`Kind::Style`] value names: a built-in word or an own style's ID.
    pub fn style(&self) -> Option<crate::StyleRef> {
        match *self {
            Value::Text(t) => crate::StyleRef::builtin(t),
            Value::OwnStyle(id) => Some(crate::StyleRef::Own(id)),
            Value::Length(_)
            | Value::Number(_)
            | Value::Choice(_)
            | Value::Flag(_)
            | Value::Size(_)
            | Value::Colours(_)
            | Value::Colour(_)
            | Value::Sheet(_)
            | Value::Margins(_)
            | Value::CoreColours(_) => None,
        }
    }

    /// The value a style reference is as a [`Kind::Style`] value.
    pub fn of_style(id: &crate::StyleRef) -> Value<'_> {
        match id {
            crate::StyleRef::Builtin(w) => Value::Text(w.as_str()),
            crate::StyleRef::Own(n) => Value::OwnStyle(*n),
        }
    }
}

/// A level that may override an entry's built-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// The project's standard: its values, `Document.standard.values`.
    Standard,
    Project,
    Page,
    /// A component or a conductor, and a symbol through its component (`text.<rep>.<rest>`).
    Object,
}

/// Where a resolved value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    Builtin,
    Standard,
    Project,
    Page,
    /// The component's (device's) or conductor's own key.
    Object,
    /// The symbol's key on its component (`text.<rep>.<rest>`).
    Symbol,
    /// A project symbol's own value in its definition: the built-in of a
    /// `symbol.<key>.*` entry of a symbol made in the project ([`symbols::resolve_for`]).
    Definition,
}

impl Source {
    /// The word `defaults` and the Properties panel use.
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Builtin => "builtin",
            Source::Standard => "standard",
            Source::Project => "project",
            Source::Page => "page",
            Source::Object => "object",
            Source::Symbol => "symbol",
            Source::Definition => "definition",
        }
    }
}

/// Where the project level of an entry is stored. One path only: an entry backed by a setting
/// never also reads a project property.
#[derive(Clone, Copy)]
pub enum ProjectSource {
    /// The project property named by the entry's key.
    Property,
    /// A project setting: its value (a length in millimetres, a number, a word of the entry's
    /// choices, or text), or `None` when the project leaves it at its default, so it resolves as
    /// the built-in. The value is checked against the entry's kind like a stored one.
    Settings(for<'a> fn(&'a ProjectSettings) -> Option<Value<'a>>),
}

impl std::fmt::Debug for ProjectSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectSource::Property => f.write_str("Property"),
            ProjectSource::Settings(_) => f.write_str("Settings"),
        }
    }
}

/// The objects [`resolve`] looks at. Empty ([`Scope::project`]) looks at the project only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Scope {
    pub page: Option<PageId>,
    pub component: Option<DeviceId>,
    pub rep: Option<RepresentationId>,
    pub conductor: Option<WireId>,
}

impl Scope {
    /// The project only.
    pub fn project() -> Self {
        Scope::default()
    }

    /// A page.
    pub fn page(page: PageId) -> Self {
        Scope { page: Some(page), ..Scope::default() }
    }

    /// A component (device), with no page.
    pub fn component(component: DeviceId) -> Self {
        Scope { component: Some(component), ..Scope::default() }
    }

    /// A symbol: the representation, its component and its page (looked up in `doc`; an unknown
    /// representation gives just the representation, whose keys are then never found).
    pub fn symbol(doc: &Document, rep: RepresentationId) -> Self {
        let r = doc.representations().get(&rep);
        let component = r.and_then(|r| match r.subject {
            Subject::Device(c) => Some(c),
            _ => None,
        });
        Scope { page: r.map(|r| r.page), component, rep: Some(rep), conductor: None }
    }
}

/// A resolved value and where it came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resolved<'a> {
    pub value: Value<'a>,
    pub from: Source,
}

impl Resolved<'_> {
    /// A length in drawing units: millimetres, or inches in an inch project. A non-length value
    /// (never, for a [`Kind::Length`] entry) gives 0.
    pub fn length(&self, doc: &Document) -> f64 {
        match self.value {
            Value::Length(mm) => match doc.settings().length_unit {
                LengthUnit::Millimetre => mm,
                LengthUnit::Inch => mm / 25.4,
            },
            _ => 0.0,
        }
    }

    /// A number (a [`Kind::Number`] entry; a length gives its millimetres); 0 for a word or text.
    pub fn number(&self) -> f64 {
        match self.value {
            Value::Number(x) | Value::Length(x) => x,
            _ => 0.0,
        }
    }

    /// The value of a [`Kind::Choice`] entry declared as `E::WORDS`; `None` for any other entry.
    pub fn choice<E: crate::Word>(&self) -> Option<E> {
        E::parse(self.word())
    }

    /// Whether a [`Kind::Flag`] entry is on; `false` for any other entry.
    pub fn flag(&self) -> bool {
        self.value == Value::Flag(true)
    }

    /// A word (a [`Kind::Choice`] entry, or a flag's `yes`/`no`); `""` otherwise.
    pub fn word(&self) -> &'static str {
        match self.value {
            Value::Choice(w) => w,
            Value::Flag(b) => crate::naming::words::flag_word(b),
            _ => "",
        }
    }

    /// The value as text: a word, a text, or a number as written.
    pub fn text(&self) -> String {
        match self.value {
            Value::Length(x) | Value::Number(x) => format!("{x}"),
            Value::Choice(w) => w.to_string(),
            Value::Flag(b) => crate::naming::words::flag_word(b).to_string(),
            Value::Text(t) => t.to_string(),
            Value::OwnStyle(id) => crate::StyleRef::Own(id).text(),
            Value::Size(s) => s.to_string(),
            Value::Colours(c) => c.to_string(),
            Value::Colour(c) => c.to_string(),
            Value::Sheet(s) => s.text(),
            Value::Margins(m) => crate::document::sheet::margins_text(&m),
            Value::CoreColours(c) => c.to_string(),
        }
    }
}

impl Entry {
    /// The built-in value, from the data files: read once, then a
    /// field read. A missing or damaged built-in gives the kind's safe value
    /// ([`data::safe_value`]), never a crash.
    pub fn builtin(&self) -> Value<'static> {
        *self.slot.get_or_init(|| self.slot.data().builtin_value(self))
    }

    /// Parse a typed value for this entry ([`parse_input`]): a flag in any of
    /// [`crate::naming::words::flag`]'s words, everything else as [`Entry::parse`].
    pub fn parse_input<'a>(&self, typed: &'a str) -> Option<Value<'a>> {
        parse_input(self.kind, typed)
    }

    /// Parse a stored value for this entry. Anything that does not parse, is not finite, is out of
    /// range, is an unknown word, is empty or too long, or holds a control character is `None`.
    pub fn parse<'a>(&self, stored: &'a str) -> Option<Value<'a>> {
        parse_value(self.kind, stored)
    }

    /// [`Entry::parse`], and for a style choice only an ID that names a style of its catalogue in
    /// `doc` (built in, the standard's or the project's).
    pub fn parse_in<'a>(&self, doc: &Document, stored: &'a str) -> Option<Value<'a>> {
        parse_value_in(doc, self.kind, stored)
    }

    /// Whether `value` is valid for this entry's kind (the check the registry tests run on every
    /// built-in).
    pub fn accepts(&self, value: &Value<'_>) -> bool {
        accepts(self.kind, value)
    }

    /// Whether `level` may override this entry.
    pub fn allows(&self, level: Level) -> bool {
        self.levels.contains(&level)
    }
}

/// Whether `value` is valid for `kind` (what [`Entry::accepts`] checks; style fields use it too).
pub fn accepts(kind: Kind, value: &Value<'_>) -> bool {
    {
        match (kind, value) {
            (Kind::Length { min, max }, Value::Length(x)) | (Kind::Number { min, max }, Value::Number(x)) => x.is_finite() && (min..=max).contains(x),
            (Kind::Choice(words), Value::Choice(w)) => words.contains(w),
            (Kind::Flag, Value::Flag(_)) => true,
            (Kind::Text { .. } | Kind::Parsed { .. } | Kind::Style(_), Value::Text(t)) => parse_value(kind, t) == Some(Value::Text(t)),
            (Kind::Style(_), Value::OwnStyle(_)) => true,
            // A typed style field read back: its text must pass the field's rule.
            (
                Kind::Parsed { parse, .. },
                Value::Size(_)
                | Value::Colours(_)
                | Value::Colour(_)
                | Value::Sheet(_)
                | Value::Margins(_)
                | Value::CoreColours(_)
                | Value::Number(_),
            ) => parse(&Resolved { value: *value, from: Source::Builtin }.text()),
            _ => false,
        }
    }
}

impl Entry {
    /// The word for this entry's kind in `defaults`: `length`, `number`, `word` or `text`.
    pub fn kind_word(&self) -> &'static str {
        match self.kind {
            Kind::Length { .. } => "length",
            Kind::Number { .. } => "number",
            Kind::Choice(_) => "choice",
            Kind::Flag => "flag",
            Kind::Text { .. } | Kind::Parsed { .. } => "text",
            Kind::Style(_) => "style",
        }
    }
}

/// Parse a stored value of `kind` (what [`Entry::parse`] does; style fields use it too). Anything
/// that does not parse, is not finite, is out of range, is an unknown word, is empty or too long,
/// or holds a control character is `None`.
pub fn parse_value(kind: Kind, stored: &str) -> Option<Value<'_>> {
    let s = stored.trim();
    match kind {
        Kind::Length { min, max } => number_in(s, min, max).map(Value::Length),
        Kind::Number { min, max } => number_in(s, min, max).map(Value::Number),
        Kind::Choice(words) => words.iter().find(|w| w.eq_ignore_ascii_case(s)).map(|w| Value::Choice(w)),
        // Stored strictly: the written words only, as they are.
        Kind::Flag => [true, false].into_iter().find(|b| crate::naming::words::flag_word(*b) == stored).map(Value::Flag),
        Kind::Text { max_chars } => plain_text(s, max_chars).then_some(Value::Text(s)),
        Kind::Parsed { max_chars, parse } => (plain_text(s, max_chars) && parse(s)).then_some(Value::Text(s)),
        Kind::Style(_) => match crate::StyleRef::parse(s)? {
            crate::StyleRef::Builtin(_) => Some(Value::Text(s)),
            crate::StyleRef::Own(id) => Some(Value::OwnStyle(id)),
        },
    }
}

/// Parse a value typed in a command or a box: a [`Kind::Flag`] in any of
/// [`crate::naming::words::flag`]'s words (stored as `yes`/`no`); a choice whose words include `yes`
/// and `no` (a text's Shown: `yes`, `no`, `first`) takes those the same way; any other kind as
/// [`parse_value`].
pub fn parse_input(kind: Kind, typed: &str) -> Option<Value<'_>> {
    match kind {
        Kind::Flag => crate::naming::words::flag(typed).map(Value::Flag),
        Kind::Choice(words) => parse_value(kind, typed).or_else(|| {
            let w = crate::naming::words::flag_word(crate::naming::words::flag(typed)?);
            words.contains(&w).then_some(Value::Choice(w))
        }),
        _ => parse_value(kind, typed),
    }
}

/// [`parse_value`], and for [`Kind::Style`] only an ID naming a style of its catalogue in `doc`.
pub fn parse_value_in<'a>(doc: &Document, kind: Kind, stored: &'a str) -> Option<Value<'a>> {
    let v = parse_value(kind, stored)?;
    match (kind, v) {
        (Kind::Style(cat), _) => v.style().filter(|id| styles::exists(doc, cat, id)).map(|_| v),
        _ => Some(v),
    }
}

/// 1 to `max_chars` characters, no control characters.
fn plain_text(s: &str, max_chars: usize) -> bool {
    !s.is_empty() && s.chars().count() <= max_chars && !s.chars().any(char::is_control)
}

fn number_in(s: &str, min: f64, max: f64) -> Option<f64> {
    s.parse::<f64>().ok().filter(|x| x.is_finite() && (min..=max).contains(x))
}

/// The key a symbol's own value of `key` is stored under on its component: the area, the
/// representation ID (32 hex digits), then the rest (`text.tag.show` → `text.<rep>.tag.show`).
pub fn symbol_key(key: &str, rep: RepresentationId) -> String {
    match key.split_once('.') {
        Some((area, rest)) => format!("{area}.{}.{rest}", rep.hex()),
        None => format!("{key}.{}", rep.hex()),
    }
}

/// The value of `entry` at `at` (see the module docs). Never fails; allocates at most the symbol
/// key string.
pub fn resolve<'a>(doc: &'a Document, entry: &Entry, at: Scope) -> Resolved<'a> {
    let stored = |props: &'a crate::Properties, key: &str| props.get(key).and_then(|v| entry.parse_in(doc, v.as_str()));
    if entry.allows(Level::Object) {
        if let Some(c) = at.component.and_then(|c| doc.devices().get(&c)) {
            if let Some(value) = at.rep.and_then(|rep| stored(&c.properties, &symbol_key(entry.key, rep))) {
                return Resolved { value, from: Source::Symbol };
            }
            if let Some(value) = stored(&c.properties, entry.key) {
                return Resolved { value, from: Source::Object };
            }
        }
        if let Some(value) = at.conductor.and_then(|c| doc.wires().get(&c)).and_then(|c| wire_value(doc, entry, c)) {
            return Resolved { value, from: Source::Object };
        }
    }
    if entry.allows(Level::Page)
        && let Some(value) = at.page.and_then(|p| doc.pages().get(&p)).and_then(|p| page_value(doc, entry, p))
    {
        return Resolved { value, from: Source::Page };
    }
    if entry.allows(Level::Project) {
        let value = match entry.project {
            ProjectSource::Property => stored(doc.display(), entry.key),
            ProjectSource::Settings(read) => read(doc.settings()).filter(|v| entry.accepts(v)),
        };
        if let Some(value) = value {
            return Resolved { value, from: Source::Project };
        }
    }
    if entry.allows(Level::Standard)
        && let Some(value) = standard_value(doc, entry)
    {
        return Resolved { value, from: Source::Standard };
    }
    Resolved { value: entry.builtin(), from: Source::Builtin }
}

/// `entry`'s value stored on `page`: its display bag, or for the page type the page's field,
/// read with the same rule as `crate::document::sheet::page_type_of` (a type that
/// does not exist reads as absent), so the two never disagree.
fn page_value<'a>(doc: &Document, entry: &Entry, page: &'a crate::Page) -> Option<Value<'a>> {
    if entry.key == crate::document::sheet::PAGE_TYPE_KEY {
        return page.page_type.as_ref().filter(|id| crate::document::sheet::page_type_exists(doc, id)).map(Value::of_style);
    }
    page.display.get(entry.key).and_then(|v| entry.parse_in(doc, v.as_str()))
}

/// `entry`'s value stored on wire `w`: its display bag, or for the wire type its field. A type that
/// does not exist reads as absent.
pub fn wire_value<'a>(doc: &Document, entry: &Entry, w: &'a crate::Wire) -> Option<Value<'a>> {
    if entry.key == crate::connectivity::wiring::WIRE_CATEGORY
        && let Kind::Style(cat) = entry.kind
    {
        return w.wire_type.as_ref().filter(|id| styles::exists(doc, cat, id)).map(Value::of_style);
    }
    w.display.get(entry.key).and_then(|v| entry.parse_in(doc, v.as_str()))
}

/// The standard's valid value of `entry`, if the project has a standard that sets one.
pub(crate) fn standard_value<'a>(doc: &'a Document, entry: &Entry) -> Option<Value<'a>> {
    doc.standard()?.values.get(entry.key).and_then(|v| entry.parse_in(doc, v.as_str()))
}

/// The chain of `entry` at `at`, broadest first: the built-in, then each
/// level that holds a value at `at` or that the entry allows, with the value it holds
/// there, if any. The standard (reserved: IEC, built in) holds none of its own yet. The object
/// level is the symbol's own value when `at` names a representation and it holds one, else the
/// component's or conductor's. [`resolve`] uses the narrowest level that holds a valid value. The
/// standard's value is its own, when the project has a standard.
/// Read only; allocates the list and at most one symbol key.
pub fn chain<'a>(doc: &'a Document, entry: &Entry, at: Scope) -> Vec<(Source, Option<Value<'a>>)> {
    let stored = |props: &'a crate::Properties, key: &str| props.get(key).and_then(|v| entry.parse_in(doc, v.as_str()));
    let mut out = vec![(Source::Builtin, Some(entry.builtin()))];
    if entry.allows(Level::Standard) {
        out.push((Source::Standard, standard_value(doc, entry)));
    }
    let project = match entry.project {
        ProjectSource::Property => stored(doc.display(), entry.key),
        ProjectSource::Settings(read) => read(doc.settings()).filter(|v| entry.accepts(v)),
    };
    if entry.allows(Level::Project) || project.is_some() {
        out.push((Source::Project, project.filter(|_| entry.allows(Level::Project))));
    }
    let page = at.page.and_then(|p| doc.pages().get(&p)).and_then(|p| page_value(doc, entry, p));
    if entry.allows(Level::Page) || page.is_some() {
        out.push((Source::Page, page.filter(|_| entry.allows(Level::Page))));
    }
    let component = at.component.and_then(|c| doc.devices().get(&c));
    let symbol = component.zip(at.rep).and_then(|(c, rep)| stored(&c.properties, &symbol_key(entry.key, rep)));
    let object = component
        .and_then(|c| stored(&c.properties, entry.key))
        .or_else(|| at.conductor.and_then(|c| doc.wires().get(&c)).and_then(|c| wire_value(doc, entry, c)));
    if entry.allows(Level::Object) || object.is_some() {
        out.push((Source::Object, object.filter(|_| entry.allows(Level::Object))));
    }
    if at.rep.is_some() && (entry.allows(Level::Object) || symbol.is_some()) {
        out.push((Source::Symbol, symbol.filter(|_| entry.allows(Level::Object))));
    }
    out
}
