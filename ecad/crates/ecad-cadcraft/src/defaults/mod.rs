//! The defaults registry (design 031 §3.5, owner principle 2026-11-12).
//!
//! Every default a v0.15 lane adds (a size, a weight, a show rule, a gap…) is **one entry** here.
//! Code that needs the value asks [`resolve`] (or a typed reader over it, such as
//! [`crate::text_style::text_style`]); it never writes the number where it is used. The read-only
//! `defaults` command and Settings ▸ Standards (brief 042) list the entries; `setdefault`
//! ([`crate::default_cmds`]) writes them at a level.
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
//! page → project (the project property `key`, or [`ProjectSource::Settings`]) → standard
//! (reserved: no source yet) → the built-in. A stored value that does not parse or is out of
//! range counts as absent, so a hostile file never gives a value outside the entry's range.

use ecad_model::{ComponentId, ConductorId, Document, LengthUnit, PageId, ProjectSettings, RepresentationId, Subject};

/// The areas a key may start with (overseer ruling, 2026-11-12): `text.`, `wiring.`, `wire.`,
/// `tags.`, `revisions.`, `print.`, `symbol.`, `terminal.` (brief 036), `connector.` (brief 044),
/// `strippage.` (brief 046, design 046 §7.2), `bom.` (brief 047) and `continuation.` (brief 049). Keys already approved before
/// `strippage.` (brief 046, design 046 §7.2), `bom.` (brief 047) and `plc.` (brief 045). Keys already approved before
/// the registry keep their names; the key-rule test reads this one list.
/// One area per line (review of #81, 3): lanes add a line each, and `ecad/.gitattributes` merges
/// this file with `merge=union`.
#[rustfmt::skip]
pub const AREAS: &[&str] = &[
    "text",
    "wiring",
    "wire",
    "tags",
    "revisions",
    "print",
    "symbol",
    "terminal",
    "connector",
    "strippage",
    "bom",
    "continuation",
    "plc",
];

pub mod bom;
pub mod connectors;
pub mod continuations;
pub mod plc;
pub mod print;
pub mod strip_pages;
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
    /// The built-in value; valid for `kind` (a test proves it).
    pub builtin: Value<'static>,
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
    /// Free text of 1 to this many characters, no control characters.
    Text { max_chars: usize },
    /// Text of 1 to `max_chars` characters, no control characters, that `parse` accepts (a
    /// declared parser, e.g. a tag prefix or a revision pattern).
    Parsed { max_chars: usize, parse: fn(&str) -> bool },
}

/// A value of an entry: its built-in, or a stored value that parsed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value<'a> {
    /// Millimetres.
    Length(f64),
    Number(f64),
    /// One of a [`Kind::Choice`] entry's words.
    Choice(&'static str),
    /// A [`Kind::Text`] or [`Kind::Parsed`] value.
    Text(&'a str),
}

/// A level that may override an entry's built-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// A named drawing standard. Reserved: no source until the Standards brief.
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
    pub component: Option<ComponentId>,
    pub rep: Option<RepresentationId>,
    pub conductor: Option<ConductorId>,
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
    pub fn component(component: ComponentId) -> Self {
        Scope { component: Some(component), ..Scope::default() }
    }

    /// A symbol: the representation, its component and its page (looked up in `doc`; an unknown
    /// representation gives just the representation, whose keys are then never found).
    pub fn symbol(doc: &Document, rep: RepresentationId) -> Self {
        let r = doc.representations.get(&rep);
        let component = r.and_then(|r| match r.subject {
            Subject::Component(c) => Some(c),
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

    /// A word (a [`Kind::Choice`] entry); `""` otherwise.
    pub fn word(&self) -> &'static str {
        match self.value {
            Value::Choice(w) => w,
            _ => "",
        }
    }

    /// The value as text: a word, a text, or a number as written.
    pub fn text(&self) -> String {
        match self.value {
            Value::Length(x) | Value::Number(x) => format!("{x}"),
            Value::Choice(w) => w.to_string(),
            Value::Text(t) => t.to_string(),
        }
    }
}

impl Entry {
    /// Parse a stored value for this entry. Anything that does not parse, is not finite, is out of
    /// range, is an unknown word, is empty or too long, or holds a control character is `None`.
    pub fn parse<'a>(&self, stored: &'a str) -> Option<Value<'a>> {
        let s = stored.trim();
        match self.kind {
            Kind::Length { min, max } => number_in(s, min, max).map(Value::Length),
            Kind::Number { min, max } => number_in(s, min, max).map(Value::Number),
            Kind::Choice(words) => words.iter().find(|w| w.eq_ignore_ascii_case(s)).map(|w| Value::Choice(w)),
            Kind::Text { max_chars } => plain_text(s, max_chars).then_some(Value::Text(s)),
            Kind::Parsed { max_chars, parse } => (plain_text(s, max_chars) && parse(s)).then_some(Value::Text(s)),
        }
    }

    /// Whether `value` is valid for this entry's kind (the check the registry tests run on every
    /// built-in).
    pub fn accepts(&self, value: &Value<'_>) -> bool {
        match (self.kind, value) {
            (Kind::Length { min, max }, Value::Length(x)) | (Kind::Number { min, max }, Value::Number(x)) => x.is_finite() && (min..=max).contains(x),
            (Kind::Choice(words), Value::Choice(w)) => words.contains(w),
            (Kind::Text { .. } | Kind::Parsed { .. }, Value::Text(t)) => self.parse(t) == Some(Value::Text(t)),
            _ => false,
        }
    }

    /// Whether `level` may override this entry.
    pub fn allows(&self, level: Level) -> bool {
        self.levels.contains(&level)
    }

    /// The word for this entry's kind in `defaults`: `length`, `number`, `word` or `text`.
    pub fn kind_word(&self) -> &'static str {
        match self.kind {
            Kind::Length { .. } => "length",
            Kind::Number { .. } => "number",
            Kind::Choice(_) => "choice",
            Kind::Text { .. } | Kind::Parsed { .. } => "text",
        }
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
        continuations::ENTRIES,
        plc::ENTRIES,
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
    let stored = |props: &'a ecad_model::Properties, key: &str| props.get(key).and_then(|v| entry.parse(v.as_str()));
    if entry.allows(Level::Object) {
        if let Some(c) = at.component.and_then(|c| doc.components.get(&c)) {
            if let Some(value) = at.rep.and_then(|rep| stored(&c.properties, &symbol_key(entry.key, rep))) {
                return Resolved { value, from: Source::Symbol };
            }
            if let Some(value) = stored(&c.properties, entry.key) {
                return Resolved { value, from: Source::Object };
            }
        }
        if let Some(value) = at.conductor.and_then(|c| doc.conductors.get(&c)).and_then(|c| stored(&c.properties, entry.key)) {
            return Resolved { value, from: Source::Object };
        }
    }
    if entry.allows(Level::Page)
        && let Some(value) = at.page.and_then(|p| doc.pages.get(&p)).and_then(|p| stored(&p.properties, entry.key))
    {
        return Resolved { value, from: Source::Page };
    }
    if entry.allows(Level::Project) {
        let value = match entry.project {
            ProjectSource::Property => stored(&doc.properties, entry.key),
            ProjectSource::Settings(read) => read(&doc.settings).filter(|v| entry.accepts(v)),
        };
        if let Some(value) = value {
            return Resolved { value, from: Source::Project };
        }
    }
    // Level::Standard: reserved, no source until the Standards brief.
    Resolved { value: entry.builtin, from: Source::Builtin }
}

/// The chain of `entry` at `at`, broadest first (brief 042, screen e): the built-in, then each
/// level that holds a value at `at` or that the entry allows (push-back 8), with the value it holds
/// there, if any. The standard (reserved: IEC, built in) holds none of its own yet. The object
/// level is the symbol's own value when `at` names a representation and it holds one, else the
/// component's or conductor's. [`resolve`] uses the narrowest level that holds a valid value.
/// Read only; allocates the list and at most one symbol key.
pub fn chain<'a>(doc: &'a Document, entry: &Entry, at: Scope) -> Vec<(Source, Option<Value<'a>>)> {
    let stored = |props: &'a ecad_model::Properties, key: &str| props.get(key).and_then(|v| entry.parse(v.as_str()));
    let mut out = vec![(Source::Builtin, Some(entry.builtin))];
    if entry.allows(Level::Standard) {
        out.push((Source::Standard, None));
    }
    let project = match entry.project {
        ProjectSource::Property => stored(&doc.properties, entry.key),
        ProjectSource::Settings(read) => read(&doc.settings).filter(|v| entry.accepts(v)),
    };
    if entry.allows(Level::Project) || project.is_some() {
        out.push((Source::Project, project.filter(|_| entry.allows(Level::Project))));
    }
    let page = at.page.and_then(|p| doc.pages.get(&p)).and_then(|p| stored(&p.properties, entry.key));
    if entry.allows(Level::Page) || page.is_some() {
        out.push((Source::Page, page.filter(|_| entry.allows(Level::Page))));
    }
    let component = at.component.and_then(|c| doc.components.get(&c));
    let symbol = component.zip(at.rep).and_then(|(c, rep)| stored(&c.properties, &symbol_key(entry.key, rep)));
    let object = component
        .and_then(|c| stored(&c.properties, entry.key))
        .or_else(|| at.conductor.and_then(|c| doc.conductors.get(&c)).and_then(|c| stored(&c.properties, entry.key)));
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
            .components
            .values()
            .filter_map(|c| c.properties.get(ecad_model::tag::TAG_KEY))
            .filter(|t| {
                let t = t.trim_start_matches('-').to_ascii_uppercase();
                t.strip_prefix(px.as_str()).is_some_and(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '?'))
            })
            .count();
    }
    let own = |props: &ecad_model::Properties| entry.allows(Level::Object) && props.get(entry.key).is_some_and(|v| entry.parse(v.as_str()).is_some());
    match users_noun(entry) {
        "devices" => doc.components.values().filter(|c| !own(&c.properties)).count(),
        "wires" => doc.conductors.values().filter(|c| !own(&c.properties)).count(),
        _ => doc.pages.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::{Component, Page};

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
            assert!(e.accepts(&e.builtin), "{}: built-in {:?} is not a valid {:?}", e.key, e.builtin, e.kind);
            // The built-in, written as text, parses back to itself.
            let r = Resolved { value: e.builtin, from: Source::Builtin };
            assert_eq!(e.parse(&r.text()), Some(e.builtin), "{}", e.key);
        }
    }

    #[test]
    fn hostile_stored_values_fall_through_to_the_next_level() {
        let tag = &text::TAG_HEIGHT;
        let font = &text::FONT;
        let show = &text::TAG_SHOW;
        let long = "x".repeat(10_000);
        for bad in ["NaN", "nan", "inf", "-inf", "1e308", "0.1", "11", "abc", "", "  ", "2.5mm", long.as_str()] {
            assert_eq!(tag.parse(bad), None, "height {bad:?}");
            let mut doc = Document::default();
            doc.properties.insert(tag.key.into(), bad.into());
            assert_eq!(resolve(&doc, tag, Scope::project()), Resolved { value: tag.builtin, from: Source::Builtin });
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
        doc.pages.insert(page, Page::new("p"));
        doc.properties.insert(show.key.into(), "off".into());
        if let Some(p) = doc.pages.get_mut(&page) {
            p.properties.insert(show.key.into(), "sideways".into());
        }
        assert_eq!(resolve(&doc, show, Scope::page(page)), Resolved { value: Value::Choice("off"), from: Source::Project });
    }

    #[test]
    fn the_narrowest_allowed_level_wins() {
        let show = &text::TAG_SHOW;
        let mut doc = Document::default();
        let page = PageId(1);
        let comp = ComponentId(2);
        let rep = RepresentationId(3);
        doc.pages.insert(page, Page::new("p"));
        doc.components.insert(comp, Component { properties: Default::default(), definition: None });
        let at = Scope { page: Some(page), component: Some(comp), rep: Some(rep), conductor: None };
        assert_eq!(resolve(&doc, show, at).from, Source::Builtin);
        doc.properties.insert(show.key.into(), "off".into());
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("off"), from: Source::Project });
        doc.pages.get_mut(&page).map(|p| p.properties.insert(show.key.into(), "on".into()));
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("on"), from: Source::Page });
        doc.components.get_mut(&comp).map(|c| c.properties.insert(show.key.into(), "off".into()));
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("off"), from: Source::Object });
        doc.components.get_mut(&comp).map(|c| c.properties.insert(symbol_key(show.key, rep), "first".into()));
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("first"), from: Source::Symbol });
        // The project-only scope still sees only the project.
        assert_eq!(resolve(&doc, show, Scope::project()).from, Source::Project);
        // An entry that does not allow the page level ignores a page value.
        let h = &text::TAG_HEIGHT;
        doc.pages.get_mut(&page).map(|p| p.properties.insert(h.key.into(), "4".into()));
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
        doc.settings.wiring.label_height = 2.0;
        assert_eq!(resolve(&doc, e, Scope::project()), Resolved { value: Value::Length(2.0), from: Source::Project });
        doc.settings.wiring.label_height = f64::NAN;
        assert_eq!(resolve(&doc, e, Scope::project()).from, Source::Builtin);
    }
}

#[cfg(test)]
mod kind_tests {
    use super::*;

    fn prefix(s: &str) -> bool {
        s.chars().all(|c| c.is_ascii_uppercase())
    }

    static PARSED: Entry = Entry {
        key: "tags.test.prefix",
        label: "Test prefix",
        group: "Test",
        brief: "031",
        kind: Kind::Parsed { max_chars: 3, parse: prefix },
        builtin: Value::Text("K"),
        levels: &[Level::Project],
        project: ProjectSource::Property,
    };

    fn rule(s: &ProjectSettings) -> Option<Value<'_>> {
        (s.tag_padding == 3).then_some(Value::Choice("each"))
    }

    static CHOICE: Entry = Entry {
        key: "wiring.test.rule",
        label: "Test rule",
        group: "Test",
        brief: "031",
        kind: Kind::Choice(&["net", "each"]),
        builtin: Value::Choice("net"),
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
        builtin: Value::Length(1.0),
        levels: &[Level::Page],
        project: ProjectSource::Property,
    };

    #[test]
    fn a_parsed_text_kind_checks_its_parser() {
        assert_eq!(PARSED.parse("KA"), Some(Value::Text("KA")));
        assert_eq!(PARSED.parse("ka"), None);
        assert_eq!(PARSED.parse("KAAA"), None);
        assert!(PARSED.accepts(&PARSED.builtin));
        let mut doc = Document::default();
        doc.properties.insert(PARSED.key.into(), "x1".into());
        assert_eq!(resolve(&doc, &PARSED, Scope::project()).from, Source::Builtin);
    }

    #[test]
    fn a_word_valued_setting_is_the_project_level() {
        let mut doc = Document::default();
        assert_eq!(resolve(&doc, &CHOICE, Scope::project()), Resolved { value: Value::Choice("net"), from: Source::Builtin });
        // A property of the same key is never a second project path.
        doc.properties.insert(CHOICE.key.into(), "each".into());
        assert_eq!(resolve(&doc, &CHOICE, Scope::project()).from, Source::Builtin);
        doc.settings.tag_padding = 3;
        assert_eq!(resolve(&doc, &CHOICE, Scope::project()), Resolved { value: Value::Choice("each"), from: Source::Project });
    }

    #[test]
    fn an_entry_may_declare_fewer_levels() {
        let mut doc = Document::default();
        doc.properties.insert(PAGE_ONLY.key.into(), "5".into());
        assert_eq!(resolve(&doc, &PAGE_ONLY, Scope::project()).from, Source::Builtin);
        let page = PageId(9);
        doc.pages.insert(page, ecad_model::Page::new("p"));
        doc.pages.get_mut(&page).map(|p| p.properties.insert(PAGE_ONLY.key.into(), "5".into()));
        assert_eq!(resolve(&doc, &PAGE_ONLY, Scope::page(page)), Resolved { value: Value::Length(5.0), from: Source::Page });
    }

    #[test]
    fn a_default_used_below_this_crate_is_its_model_constant() {
        assert_eq!(text::WIRE_LABEL_HEIGHT.builtin, Value::Length(ecad_model::wiring::DEFAULT_LABEL_HEIGHT));
    }
}
