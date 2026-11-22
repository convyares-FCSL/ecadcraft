//! The defaults registry (design 031 §3.5, owner principle 2026-11-12).
//!
//! Every default a v0.15 lane adds (a size, a weight, a show rule, a gap…) is **one entry** here.
//! Code that needs the value asks [`resolve`] (or a typed reader over it, such as
//! [`crate::styles::text_style::text_style`]); it never writes the number where it is used. The read-only
//! `defaults` command and Settings ▸ Standards (brief 042) list the entries; `setdefault`
//! (`ecad_engine::commands::default`) writes them at a level.
//!
//! Each lane owns one file and appends to its own `ENTRIES` slice: `text.rs` (lane T, 031),
//! `wires.rs` (W, 032), `print.rs` (P, 033), `symbols.rs` (S, 034). [`all`] chains them and does
//! not change when a lane adds an entry, so two lanes never edit one file.
//!
//! Not entries: hostile-input bounds (most candidates, most texts per page…). They are guards, not
//! preferences, and stay constants beside the code they guard.
//!
//! # Resolution
//!
//! Narrowest level first, only the levels the entry allows: object (a symbol's
//! `text.<rep>.<rest>` key on its component, then the component's or conductor's own `key`) →
//! page → project (the project property `key`, or [`ProjectSource::Settings`]) → standard (the
//! project's standard's values, brief 050a) → the built-in. A stored value that does not parse or
//! is out of range counts as absent, so a hostile file never gives a value outside the entry's
//! range. A style choice ([`Kind::Style`]) also counts as absent when no style of that ID exists
//! in its catalogue.

pub use data::{DefaultsError, Slot};
use ecad_model::{DeviceId, Document, LengthUnit, PageId, ProjectSettings, RepresentationId, Subject, WireId};

/// The areas a key may start with (overseer ruling, 2026-11-12): the words of
/// [`ecad_model::naming::keys::Area::REGISTRY`], in its order (design 064 §3.3). Keys already approved
/// before the registry keep their names; the key-rule test reads this one list.
pub const AREAS: &[&str] = <ecad_model::naming::keys::Area as ecad_model::Word>::WORDS.split_at(ecad_model::naming::keys::Area::REGISTRY.len()).0;

pub mod bom;
pub mod connectors;
pub mod continuations;
pub mod data;
pub mod pages;
pub mod plc;
pub mod print;
pub mod rating;
pub mod settings;
pub mod strip_pages;
pub mod styles;
pub mod symbols;
pub mod terminals;
pub mod text;
pub mod wires;

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
    /// The brief that added it: `"031"`.
    pub brief: &'static str,
    pub kind: Kind,
    /// Its built-in value, read from the data files on first use ([`Entry::builtin`], brief 066):
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
    /// A switch (design 072 F4): stored as exactly `yes` or `no`; typed as any word
    /// [`ecad_model::naming::words::flag`] reads.
    Flag,
    /// Free text of 1 to this many characters, no control characters.
    Text { max_chars: usize },
    /// Text of 1 to `max_chars` characters, no control characters, that `parse` accepts (a
    /// declared parser, e.g. a tag prefix or a revision pattern).
    Parsed { max_chars: usize, parse: fn(&str) -> bool },
    /// A style of this catalogue (brief 050a, design 050 §2.4): its ID, a built-in word or
    /// `#<32 hex>`. A stored ID naming no style of the catalogue counts as absent.
    Style(ecad_model::StyleCatalogue),
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
    /// A [`Kind::Style`] value naming an own style (a standard's or the project's) by its ID
    /// (design 072 §5.1): held as the ID, so a value read from a typed field needs no stored text.
    OwnStyle(u128),
    /// A typed style field's wire size (design 072 §8): a wire type's `size`.
    Size(ecad_model::objects::wire::WireSize),
    /// A typed style field's ordered colours (design 072 §8): a wire type's `colours`.
    Colours(&'a ecad_model::objects::wire::Colours),
    /// A typed style field's colour (design 072 §8): a text, location or frame style's.
    Colour(ecad_model::objects::styles::StyleColour),
    /// A typed style field's sheet (design 072 §8): a page type's or a frame style's own sheet.
    Sheet(ecad_model::document::sheet::SheetSpec),
    /// A typed style field's margins (design 072 §8): a frame style's.
    Margins(ecad_model::document::sheet::Margins),
    /// A typed style field's core colours (design 072 §8): a cable type's.
    CoreColours(&'a ecad_model::objects::styles::cable::CoreColours),
}

impl Value<'_> {
    /// The style a [`Kind::Style`] value names: a built-in word or an own style's ID.
    pub fn style(&self) -> Option<ecad_model::StyleRef> {
        match *self {
            Value::Text(t) => ecad_model::StyleRef::builtin(t),
            Value::OwnStyle(id) => Some(ecad_model::StyleRef::Own(id)),
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
    pub fn of_style(id: &ecad_model::StyleRef) -> Value<'_> {
        match id {
            ecad_model::StyleRef::Builtin(w) => Value::Text(w.as_str()),
            ecad_model::StyleRef::Own(n) => Value::OwnStyle(*n),
        }
    }
}

/// A level that may override an entry's built-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// The project's standard (brief 050a): its values, `Document.standard.values`.
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
    /// A project symbol's own value in its definition (brief 048, FP6): the built-in of a
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
/// never also reads a project property (overseer ruling, 2026-11-12).
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
        let r = doc.representations.get(&rep);
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
            Value::Length(mm) => match doc.settings.length_unit {
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
    pub fn choice<E: ecad_model::Word>(&self) -> Option<E> {
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
            Value::Flag(b) => ecad_model::naming::words::flag_word(b),
            _ => "",
        }
    }

    /// The value as text: a word, a text, or a number as written.
    pub fn text(&self) -> String {
        match self.value {
            Value::Length(x) | Value::Number(x) => format!("{x}"),
            Value::Choice(w) => w.to_string(),
            Value::Flag(b) => ecad_model::naming::words::flag_word(b).to_string(),
            Value::Text(t) => t.to_string(),
            Value::OwnStyle(id) => ecad_model::StyleRef::Own(id).text(),
            Value::Size(s) => s.to_string(),
            Value::Colours(c) => c.to_string(),
            Value::Colour(c) => c.to_string(),
            Value::Sheet(s) => s.text(),
            Value::Margins(m) => ecad_model::document::sheet::margins_text(&m),
            Value::CoreColours(c) => c.to_string(),
        }
    }
}

impl Entry {
    /// The built-in value, from the data files (brief 066, design 066 §3): read once, then a
    /// field read. A missing or damaged built-in gives the kind's safe value
    /// ([`data::safe_value`]), never a crash.
    pub fn builtin(&self) -> Value<'static> {
        *self.slot.get_or_init(|| data::builtin_value(self))
    }

    /// Parse a typed value for this entry ([`parse_input`]): a flag in any of
    /// [`ecad_model::naming::words::flag`]'s words, everything else as [`Entry::parse`].
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
            // A typed style field read back (design 072 §8): its text must pass the field's rule.
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
        // Stored strictly: the written words only, as they are (F4).
        Kind::Flag => [true, false].into_iter().find(|b| ecad_model::naming::words::flag_word(*b) == stored).map(Value::Flag),
        Kind::Text { max_chars } => plain_text(s, max_chars).then_some(Value::Text(s)),
        Kind::Parsed { max_chars, parse } => (plain_text(s, max_chars) && parse(s)).then_some(Value::Text(s)),
        Kind::Style(_) => match ecad_model::StyleRef::parse(s)? {
            ecad_model::StyleRef::Builtin(_) => Some(Value::Text(s)),
            ecad_model::StyleRef::Own(id) => Some(Value::OwnStyle(id)),
        },
    }
}

/// Parse a value typed in a command or a box: a [`Kind::Flag`] in any of
/// [`ecad_model::naming::words::flag`]'s words (stored as `yes`/`no`); a choice whose words include `yes`
/// and `no` (a text's Shown: `yes`, `no`, `first`) takes those the same way; any other kind as
/// [`parse_value`].
pub fn parse_input(kind: Kind, typed: &str) -> Option<Value<'_>> {
    match kind {
        Kind::Flag => ecad_model::naming::words::flag(typed).map(Value::Flag),
        Kind::Choice(words) => parse_value(kind, typed).or_else(|| {
            let w = ecad_model::naming::words::flag_word(ecad_model::naming::words::flag(typed)?);
            words.contains(&w).then_some(Value::Choice(w))
        }),
        _ => parse_value(kind, typed),
    }
}

/// [`parse_value`], and for [`Kind::Style`] only an ID naming a style of its catalogue in `doc`.
pub fn parse_value_in<'a>(doc: &Document, kind: Kind, stored: &'a str) -> Option<Value<'a>> {
    let v = parse_value(kind, stored)?;
    match (kind, v) {
        (Kind::Style(cat), _) => v.style().filter(|id| crate::styles::exists(doc, cat, id)).map(|_| v),
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

/// Every registered entry, lane by lane: one line per area (overseer ruling, 2026-11-13), so lanes
/// adding an area each add one line and merge without conflict.
#[rustfmt::skip]
pub fn all() -> impl Iterator<Item = &'static Entry> {
    [
        text::ENTRIES,
        wires::ENTRIES,
        print::ENTRIES,
        symbols::ENTRIES,
        terminals::ENTRIES,
        connectors::ENTRIES,
        strip_pages::ENTRIES,
        bom::ENTRIES,
        styles::ENTRIES,
        symbols::BUILDER_ENTRIES,
        continuations::ENTRIES,
        plc::ENTRIES,
        pages::ENTRIES,
        rating::ENTRIES,
        settings::ENTRIES,
    ]
    .into_iter()
    .flatten()
    .copied()
}

/// The entry with this key.
pub fn find(key: &str) -> Option<&'static Entry> {
    all().find(|e| e.key == key)
}

/// The value of `entry` at `at` (see the module docs). Never fails; allocates at most the symbol
/// key string.
pub fn resolve<'a>(doc: &'a Document, entry: &Entry, at: Scope) -> Resolved<'a> {
    let stored = |props: &'a ecad_model::Properties, key: &str| props.get(key).and_then(|v| entry.parse_in(doc, v.as_str()));
    if entry.allows(Level::Object) {
        if let Some(c) = at.component.and_then(|c| doc.devices.get(&c)) {
            if let Some(value) = at.rep.and_then(|rep| stored(&c.properties, &symbol_key(entry.key, rep))) {
                return Resolved { value, from: Source::Symbol };
            }
            if let Some(value) = stored(&c.properties, entry.key) {
                return Resolved { value, from: Source::Object };
            }
        }
        if let Some(value) = at.conductor.and_then(|c| doc.wires.get(&c)).and_then(|c| wire_value(doc, entry, c)) {
            return Resolved { value, from: Source::Object };
        }
    }
    if entry.allows(Level::Page)
        && let Some(value) = at.page.and_then(|p| doc.pages.get(&p)).and_then(|p| page_value(doc, entry, p))
    {
        return Resolved { value, from: Source::Page };
    }
    if entry.allows(Level::Project) {
        let value = match entry.project {
            ProjectSource::Property => stored(&doc.display, entry.key),
            ProjectSource::Settings(read) => read(&doc.settings).filter(|v| entry.accepts(v)),
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

/// `entry`'s value stored on `page`: its display bag, or for the page type the page's field
/// (design 072 §5.1), read with the same rule as `ecad_model::document::sheet::page_type_of` (a type that
/// does not exist reads as absent), so the two never disagree.
fn page_value<'a>(doc: &Document, entry: &Entry, page: &'a ecad_model::Page) -> Option<Value<'a>> {
    if entry.key == ecad_model::document::sheet::PAGE_TYPE_KEY {
        return page.page_type.as_ref().filter(|id| ecad_model::document::sheet::page_type_exists(doc, id)).map(Value::of_style);
    }
    page.display.get(entry.key).and_then(|v| entry.parse_in(doc, v.as_str()))
}

/// `entry`'s value stored on wire `w`: its display bag, or for the wire type its field (design
/// 072 §4.6); a type that does not exist reads as absent.
fn wire_value<'a>(doc: &Document, entry: &Entry, w: &'a ecad_model::Wire) -> Option<Value<'a>> {
    if entry.key == ecad_model::connectivity::wiring::WIRE_CATEGORY {
        return w.wire_type.as_ref().filter(|id| crate::styles::exists(doc, ecad_model::StyleCatalogue::Wire, id)).map(Value::of_style);
    }
    w.display.get(entry.key).and_then(|v| entry.parse_in(doc, v.as_str()))
}

/// The standard's valid value of `entry`, if the project has a standard that sets one (brief
/// 050a, design 050 §2.4).
pub fn standard_value<'a>(doc: &'a Document, entry: &Entry) -> Option<Value<'a>> {
    doc.standard.as_ref()?.values.get(entry.key).and_then(|v| entry.parse_in(doc, v.as_str()))
}

/// The chain of `entry` at `at`, broadest first (brief 042, screen e): the built-in, then each
/// level that holds a value at `at` or that the entry allows (push-back 8), with the value it holds
/// there, if any. The standard (reserved: IEC, built in) holds none of its own yet. The object
/// level is the symbol's own value when `at` names a representation and it holds one, else the
/// component's or conductor's. [`resolve`] uses the narrowest level that holds a valid value. The
/// standard's value is its own (brief 050a), when the project has a standard.
/// Read only; allocates the list and at most one symbol key.
pub fn chain<'a>(doc: &'a Document, entry: &Entry, at: Scope) -> Vec<(Source, Option<Value<'a>>)> {
    let stored = |props: &'a ecad_model::Properties, key: &str| props.get(key).and_then(|v| entry.parse_in(doc, v.as_str()));
    let mut out = vec![(Source::Builtin, Some(entry.builtin()))];
    if entry.allows(Level::Standard) {
        out.push((Source::Standard, standard_value(doc, entry)));
    }
    let project = match entry.project {
        ProjectSource::Property => stored(&doc.display, entry.key),
        ProjectSource::Settings(read) => read(&doc.settings).filter(|v| entry.accepts(v)),
    };
    if entry.allows(Level::Project) || project.is_some() {
        out.push((Source::Project, project.filter(|_| entry.allows(Level::Project))));
    }
    let page = at.page.and_then(|p| doc.pages.get(&p)).and_then(|p| page_value(doc, entry, p));
    if entry.allows(Level::Page) || page.is_some() {
        out.push((Source::Page, page.filter(|_| entry.allows(Level::Page))));
    }
    let component = at.component.and_then(|c| doc.devices.get(&c));
    let symbol = component.zip(at.rep).and_then(|(c, rep)| stored(&c.properties, &symbol_key(entry.key, rep)));
    let object = component
        .and_then(|c| stored(&c.properties, entry.key))
        .or_else(|| at.conductor.and_then(|c| doc.wires.get(&c)).and_then(|c| wire_value(doc, entry, c)));
    if entry.allows(Level::Object) || object.is_some() {
        out.push((Source::Object, object.filter(|_| entry.allows(Level::Object))));
    }
    if at.rep.is_some() && (entry.allows(Level::Object) || symbol.is_some()) {
        out.push((Source::Symbol, symbol.filter(|_| entry.allows(Level::Object))));
    }
    out
}

/// What [`users`] counts for `entry`: `"devices"`, `"wires"` or `"pages"`.
pub fn users_noun(entry: &Entry) -> &'static str {
    let area = entry.key.split('.').next().unwrap_or_default();
    // Brief 046: a page style is counted by page, though its key is in the `terminal` area.
    if entry.key == strip_pages::STYLE.key {
        return "pages";
    }
    match area {
        "text" | "symbol" | "terminal" | "tags" if entry.key != text::WIRE_LABEL_HEIGHT.key => "devices",
        "wire" | "wiring" | "text" => "wires",
        _ => "pages",
    }
}

/// How many objects use the project's value of `entry` (brief 042, "consequences said first"):
/// for a tag prefix, the devices whose tag starts with the prefix the project resolves to; for
/// another entry counted by device or wire ([`users_noun`]), the devices or conductors with no
/// value of their own; otherwise the pages. Read only; one pass over the project's devices,
/// conductors or pages (asked for one entry at a time, never per frame).
pub fn users(doc: &Document, entry: &Entry) -> usize {
    if entry.key.ends_with(".prefix") && matches!(entry.kind, Kind::Parsed { .. }) {
        let px = resolve(doc, entry, Scope::project()).text();
        let px = px.trim().trim_start_matches('-').to_ascii_uppercase();
        return doc
            .devices
            .values()
            .filter_map(|c| c.tag())
            .filter(|t| {
                let t = t.trim_start_matches('-').to_ascii_uppercase();
                t.strip_prefix(px.as_str()).is_some_and(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '?'))
            })
            .count();
    }
    // A style choice (brief 050a): the users of the style the project resolves to.
    if let Kind::Style(cat) = entry.kind
        && let Some(id) = ecad_model::StyleRef::parse(&resolve(doc, entry, Scope::project()).text())
    {
        let c = crate::styles::users(doc, cat, &id);
        return c.explicit + c.implicit;
    }
    let own = |props: &ecad_model::Properties| entry.allows(Level::Object) && props.get(entry.key).is_some_and(|v| entry.parse(v.as_str()).is_some());
    match users_noun(entry) {
        "devices" => doc.devices.values().filter(|c| !own(&c.properties)).count(),
        "wires" => doc.wires.values().filter(|c| !(entry.allows(Level::Object) && wire_value(doc, entry, c).is_some())).count(),
        _ => doc.pages.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::{Device, Page};

    fn key_ok(key: &str) -> bool {
        let parts: Vec<&str> = key.split('.').collect();
        parts.len() >= 2
            && AREAS.contains(&parts[0])
            && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
    }

    #[test]
    fn keys_are_unique_and_well_formed() {
        let mut seen = std::collections::BTreeSet::new();
        for e in all() {
            assert!(seen.insert(e.key), "duplicate key {}", e.key);
            assert!(key_ok(e.key), "bad key {}", e.key);
            assert!(!e.label.is_empty() && !e.group.is_empty() && !e.brief.is_empty(), "{} lacks words", e.key);
        }
    }

    #[test]
    fn every_builtin_is_valid_for_its_kind() {
        // Levels are per entry (an entry may declare fewer); only the built-in is always there.
        for e in all() {
            assert!(e.accepts(&e.builtin()), "{}: built-in {:?} is not a valid {:?}", e.key, e.builtin(), e.kind);
            // The built-in, written as text, parses back to itself.
            let r = Resolved { value: e.builtin(), from: Source::Builtin };
            assert_eq!(e.parse(&r.text()), Some(e.builtin()), "{}", e.key);
        }
    }

    #[test]
    fn hostile_stored_values_fall_through_to_the_next_level() {
        let tag = &text::WIRE_LABEL_HEIGHT;
        let font = &text::FONT;
        let show = &text::TAG_SHOW;
        let long = "x".repeat(10_000);
        for bad in ["NaN", "nan", "inf", "-inf", "1e308", "0.1", "11", "abc", "", "  ", "2.5mm", long.as_str()] {
            assert_eq!(tag.parse(bad), None, "height {bad:?}");
            let mut doc = Document::default();
            doc.display.insert(tag.key.into(), bad.into());
            assert_eq!(resolve(&doc, tag, Scope::project()), Resolved { value: tag.builtin(), from: Source::Builtin });
        }
        for bad in ["", "a\u{7}b", "line\nbreak", &"f".repeat(65), long.as_str()] {
            assert_eq!(font.parse(bad), None, "font {bad:?}");
        }
        for bad in ["maybe", "first ", "1", ""] {
            if bad == "first " {
                // trimmed: a valid word
                assert_eq!(show.parse(bad), Some(Value::Choice("first")));
            } else {
                assert_eq!(show.parse(bad), None, "show {bad:?}");
            }
        }
        // A hostile page value under a good project value: the project wins.
        let mut doc = Document::default();
        let page = PageId(7);
        doc.pages.insert(page, Page::titled("p").unwrap());
        doc.display.insert(show.key.into(), "no".into());
        if let Some(p) = doc.pages.get_mut(&page) {
            p.display.insert(show.key.into(), "sideways".into());
        }
        assert_eq!(resolve(&doc, show, Scope::page(page)), Resolved { value: Value::Choice("no"), from: Source::Project });
    }

    #[test]
    fn the_narrowest_allowed_level_wins() {
        let show = &text::TAG_SHOW;
        let mut doc = Document::default();
        let page = PageId(1);
        let comp = DeviceId(2);
        let rep = RepresentationId(3);
        doc.pages.insert(page, Page::titled("p").unwrap());
        doc.devices.insert(comp, Device::default());
        let at = Scope { page: Some(page), component: Some(comp), rep: Some(rep), conductor: None };
        assert_eq!(resolve(&doc, show, at).from, Source::Builtin);
        doc.display.insert(show.key.into(), "no".into());
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("no"), from: Source::Project });
        doc.pages.get_mut(&page).map(|p| p.display.insert(show.key.into(), "yes".into()));
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("yes"), from: Source::Page });
        doc.devices.get_mut(&comp).map(|c| c.properties.insert(show.key.into(), "no".into()));
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("no"), from: Source::Object });
        doc.devices.get_mut(&comp).map(|c| c.properties.insert(symbol_key(show.key, rep), "first".into()));
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("first"), from: Source::Symbol });
        // The project-only scope still sees only the project.
        assert_eq!(resolve(&doc, show, Scope::project()).from, Source::Project);
        // An entry that does not allow the page level ignores a page value.
        let h = &text::RUN_ALIGN;
        doc.pages.get_mut(&page).map(|p| p.display.insert(h.key.into(), "4".into()));
        assert_eq!(resolve(&doc, h, at).from, Source::Builtin);
    }

    #[test]
    fn symbol_keys_put_the_representation_after_the_area() {
        let rep = RepresentationId(0xab);
        assert_eq!(symbol_key("text.tag.show", rep), format!("text.{:032x}.tag.show", 0xab));
    }

    #[test]
    fn settings_sources_resolve_as_the_project_level() {
        let e = &text::WIRE_LABEL_HEIGHT;
        let mut doc = Document::default();
        assert_eq!(resolve(&doc, e, Scope::project()).from, Source::Builtin);
        doc.settings.wiring.label_height = Some(2.0);
        assert_eq!(resolve(&doc, e, Scope::project()), Resolved { value: Value::Length(2.0), from: Source::Project });
        doc.settings.wiring.label_height = Some(f64::NAN);
        assert_eq!(resolve(&doc, e, Scope::project()).from, Source::Builtin);
    }
}

#[cfg(test)]
mod kind_tests {
    use super::*;

    /// Design 072 A2: each choice declared as a word enum keeps the words, in the order, of the
    /// list it replaced (the first word is the entry's fallback).
    #[test]
    fn choice_enums_keep_their_words() {
        let table: &[(&str, &[&str])] = &[
            ("connector.maker.halves", &["socket", "plug", "both"]),
            ("connector.maker.family", &["plain", "dsub", "m12", "rj45", "usba", "usbb"]),
            ("connector.maker.names", &["numbers", "letters", "list"]),
            ("connector.maker.layout", &["row", "pins"]),
            ("plc.slot.first", &["0", "1"]),
            ("plc.channel.first", &["0", "1"]),
            ("plc.pins", &["role", "channel"]),
            ("plc.address.style", &["byte.bit", "iec", "percent", "slot", "none"]),
            ("plc.text.channel.rotation", &["90", "0"]),
            ("plc.safe.dq", &["off", "on", "hold"]),
            ("print.continuation_numbers", &["sequence"]),
            ("print.missing_mark", &["boxed"]),
            ("print.contact_table_fallback", &["right_then_lift"]),
            (crate::releases::revisions::PENDING_KEY, &["star", "draft", "off"]),
            ("print.colour", &["mono", "colour"]),
            ("rating.system", &["ac1", "ac3", "dc"]),
            ("terminal.plan.style", &["table", "graphic"]),
            ("bom.lines", &["flat", "nested"]),
        ];
        for (key, want) in table {
            let e = find(key);
            assert!(matches!(e.map(|e| e.kind), Some(Kind::Choice(w)) if w == *want), "{key}");
        }
        assert_eq!(<crate::styles::frame::BlockAt as ecad_model::Word>::WORDS, ["bottom_full", "bottom_right"]);
        assert_eq!(<crate::styles::text_style::Weight as ecad_model::Word>::WORDS, ["regular", "bold"]);
        assert_eq!(<crate::styles::wire::LineType as ecad_model::Word>::WORDS, ["continuous", "dashed", "dashdot"]);
        assert_eq!(<crate::styles::wire::RouteLabels as ecad_model::Word>::WORDS, ["project", "net", "each", "none"]);
        assert_eq!(<ecad_model::objects::styles::wire::ConductorClass as ecad_model::Word>::WORDS, ["1", "2", "5", "6"]);
        assert_eq!(<ecad_model::objects::styles::wire::Material as ecad_model::Word>::WORDS, ["cu"]);
    }

    fn prefix(s: &str) -> bool {
        s.chars().all(|c| c.is_ascii_uppercase())
    }

    static PARSED: Entry = Entry {
        key: "tags.test.prefix",
        label: "Test prefix",
        group: "Test",
        brief: "031",
        kind: Kind::Parsed { max_chars: 3, parse: prefix },
        slot: Slot::new(),
        levels: &[Level::Project],
        project: ProjectSource::Property,
    };

    fn rule(s: &ProjectSettings) -> Option<Value<'_>> {
        (s.tag_padding == Some(3)).then_some(Value::Choice("each"))
    }

    static CHOICE: Entry = Entry {
        key: "wiring.test.rule",
        label: "Test rule",
        group: "Test",
        brief: "031",
        kind: Kind::Choice(&["net", "each"]),
        slot: Slot::new(),
        levels: &[Level::Standard, Level::Project],
        project: ProjectSource::Settings(rule),
    };

    /// An entry with no project level ignores a project value.
    static PAGE_ONLY: Entry = Entry {
        key: "print.test.page",
        label: "Test page only",
        group: "Test",
        brief: "031",
        kind: Kind::Length { min: 0.0, max: 10.0 },
        slot: Slot::new(),
        levels: &[Level::Page],
        project: ProjectSource::Property,
    };

    #[test]
    fn a_parsed_text_kind_checks_its_parser() {
        assert_eq!(PARSED.parse("KA"), Some(Value::Text("KA")));
        assert_eq!(PARSED.parse("ka"), None);
        assert_eq!(PARSED.parse("KAAA"), None);
        let mut doc = Document::default();
        doc.display.insert(PARSED.key.into(), "x1".into());
        assert_eq!(resolve(&doc, &PARSED, Scope::project()).from, Source::Builtin);
    }

    #[test]
    fn a_word_valued_setting_is_the_project_level() {
        let mut doc = Document::default();
        assert_eq!(resolve(&doc, &CHOICE, Scope::project()), Resolved { value: Value::Choice("net"), from: Source::Builtin });
        // A property of the same key is never a second project path.
        doc.display.insert(CHOICE.key.into(), "each".into());
        assert_eq!(resolve(&doc, &CHOICE, Scope::project()).from, Source::Builtin);
        doc.settings.tag_padding = Some(3);
        assert_eq!(resolve(&doc, &CHOICE, Scope::project()), Resolved { value: Value::Choice("each"), from: Source::Project });
    }

    #[test]
    fn an_entry_may_declare_fewer_levels() {
        let mut doc = Document::default();
        doc.display.insert(PAGE_ONLY.key.into(), "5".into());
        assert_eq!(resolve(&doc, &PAGE_ONLY, Scope::project()).from, Source::Builtin);
        let page = PageId(9);
        doc.pages.insert(page, ecad_model::Page::titled("p").unwrap());
        doc.pages.get_mut(&page).map(|p| p.display.insert(PAGE_ONLY.key.into(), "5".into()));
        assert_eq!(resolve(&doc, &PAGE_ONLY, Scope::page(page)), Resolved { value: Value::Length(5.0), from: Source::Page });
    }

    #[test]
    fn an_entry_with_no_data_takes_its_safe_value() {
        // Not in any file: the kind's safe value, never a crash (design 066 §7).
        assert_eq!(CHOICE.builtin(), Value::Choice("net"));
        assert_eq!(PAGE_ONLY.builtin(), Value::Length(0.0));
    }
}
