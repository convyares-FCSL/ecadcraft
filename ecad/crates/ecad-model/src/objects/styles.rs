//! Standards and style records (brief 050a, design 050 §2–§3, FP1–FP3).
//!
//! A **standard** is a named, shareable house style that a project holds a copy of: a header,
//! registry values at the Standard level, and standard-layer style records. A **style record** is
//! what a standard or a project stores about one style of a catalogue: its name and the fields it
//! sets. A **style reference** ([`StyleRef`]) names a style by a stable ID: a built-in word
//! (`tag`, `box`, `dc24`) or an own style's `#<32 hex>`.
//!
//! This module holds only stored data and its shape rules. What a catalogue's fields mean, its
//! built-in styles and how a field resolves live in `ecad-engine` (`styles`). Excluded: library
//! origins (2.1; [`StandardHeader::origin`] is reserved and always `None`).

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::doc::Properties;
use crate::naming::id::{LibraryId, StandardId};

pub mod cable;
pub mod frame;
pub mod location;
pub mod page;
pub mod rating;
pub mod text;
pub mod wire;

pub use cable::CableType;
pub use frame::FrameStyle;
pub use location::LocationStyle;
pub use page::PageType;
pub use rating::RatingTable;
pub use text::{StyleColour, TextStyle};
pub use wire::WireType;

/// The most styles a layer may hold per catalogue. Guard, not a default: it keeps a hostile
/// file from making the catalogue tables and the users index unbounded. Records past it are kept
/// in the document, ignored by readers and reported by Checks.
pub const MAX_STYLES_PER_CATALOGUE: usize = 1_000;
/// The most fields one style record may hold. Guard: it keeps one record bounded. Fields past it
/// are ignored by readers and reported by Checks.
pub const MAX_FIELDS_PER_STYLE: usize = 64;
/// The longest style or standard name, in characters. Guard: names are labels in tables.
pub const MAX_NAME_CHARS: usize = 64;
/// The longest stored field value, in characters. Guard: a longer value counts as absent.
pub const MAX_FIELD_CHARS: usize = 256;
/// The longest catalogue word or built-in style word, in characters.
pub const MAX_WORD_CHARS: usize = 32;

/// Where a style record lives (design 050 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleLayer {
    /// The project's standard (only when it has one).
    Standard,
    /// The project itself.
    Project,
}

impl StyleLayer {
    /// Its word: `standard` or `project`.
    pub fn word(self) -> &'static str {
        match self {
            StyleLayer::Standard => "standard",
            StyleLayer::Project => "project",
        }
    }
}

crate::word_enum! {
    /// A style catalogue (design 050 §2, design 072 F1, F2): which kind of style a record or a
    /// reference is. One variant per catalogue; a later catalogue adds one.
    ///
    /// The variants are in their words' order, so a map keyed by catalogue ([`StyleCatalogues`])
    /// is written in the same order as when it was keyed by the word: same file bytes.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum StyleCatalogue {
        Cable,
        Frame,
        Location,
        Page,
        #[strum(serialize = "ratingtable")]
        RatingTable,
        Text,
        Wire,
    }
}

impl fmt::Display for StyleCatalogue {
    /// Its stored word.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(crate::naming::words::Word::word(*self))
    }
}

/// Whether `w` is a well-formed catalogue word or built-in style word: 1–32 characters of
/// `a-z 0-9 _`.
pub fn is_word(w: &str) -> bool {
    !w.is_empty() && w.len() <= MAX_WORD_CHARS && w.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

/// Whether a property `key` is a style choice (design 050 §2.5, FP5): `text.<kind>.style`,
/// `location.style`, `print.frame.style`, `wire.category` or `page.type` (brief 054, FP2). Objects
/// holding one are the style's explicit users, kept in the document's index
/// ([`crate::Document::style_users`]).
pub fn is_choice_key(key: &str) -> bool {
    matches!(key, "location.style" | "print.frame.style" | "wire.category" | "page.type")
        || key.strip_prefix("text.").and_then(|r| r.strip_suffix(".style")).is_some_and(is_word)
}

/// Whether `name` is a usable style or standard name: 1–64 characters, no control characters,
/// not only spaces.
pub fn is_name(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= MAX_NAME_CHARS && !name.chars().any(char::is_control)
}

/// A stable reference to a style of one catalogue (design 050 §2.2). A name is only a label.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StyleRef {
    /// A built-in style's word, unique in its catalogue; never renamed or reused.
    Builtin(String),
    /// An own style, added by a standard or a project: a 128-bit ID from the project's
    /// [`crate::IdAllocator`], the same in a standard and every project that copies it.
    Own(u128),
}

impl StyleRef {
    /// A built-in word reference, if `w` is a well-formed word.
    pub fn builtin(w: &str) -> Option<StyleRef> {
        is_word(w).then(|| StyleRef::Builtin(w.to_string()))
    }

    /// Parse the written form: a built-in word, or `#` and 32 hex digits. Surrounding spaces are
    /// ignored; anything else is `None`.
    pub fn parse(text: &str) -> Option<StyleRef> {
        let t = text.trim();
        match t.strip_prefix('#') {
            Some(hex) if hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_hexdigit()) => u128::from_str_radix(hex, 16).ok().map(StyleRef::Own),
            Some(_) => None,
            None => StyleRef::builtin(t),
        }
    }

    /// The written form (`tag`, `#00ab…`).
    pub fn text(&self) -> String {
        match self {
            StyleRef::Builtin(w) => w.clone(),
            StyleRef::Own(id) => format!("#{id:032x}"),
        }
    }

    /// Whether it names a built-in style.
    pub fn is_builtin(&self) -> bool {
        matches!(self, StyleRef::Builtin(_))
    }
}

impl From<u128> for StyleRef {
    fn from(v: u128) -> Self {
        StyleRef::Own(v)
    }
}

impl fmt::Debug for StyleRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text())
    }
}

impl fmt::Display for StyleRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text())
    }
}

impl Serialize for StyleRef {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.text())
    }
}

impl<'de> Deserialize<'de> for StyleRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        StyleRef::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("{text:?} is not a style reference (a word, or # and 32 hex digits)")))
    }
}

/// What a standard or a project stores about one style (design 050 §2.3, design 072 §8): its name
/// and its typed body. For a built-in style the name is empty and the body holds only the fields
/// it overrides.
///
/// The body's catalogue is the catalogue the record is filed under; a record filed under another
/// catalogue is refused on apply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleRecord {
    /// Its name: an own style's label; none for an override of a built-in style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<StyleName>,
    pub body: StyleBody,
}

/// A style or standard name: 1–64 characters, one line, no hidden character (design 072 §9.4).
pub type StyleName = crate::naming::text::Text<MAX_NAME_CHARS>;

impl StyleRecord {
    /// A record with `name` (none when empty or not a valid name) and `body`.
    pub fn new(name: impl AsRef<str>, body: StyleBody) -> StyleRecord {
        StyleRecord { name: StyleName::new(name.as_ref()).ok(), body }
    }

    /// An unnamed record of `catalogue` that sets nothing.
    pub fn empty(catalogue: StyleCatalogue) -> StyleRecord {
        StyleRecord { name: None, body: StyleBody::empty(catalogue) }
    }

    /// Its name's text, empty when it has none.
    pub fn name_str(&self) -> &str {
        self.name.as_ref().map_or("", |n| n.as_str())
    }

    /// Which catalogue its body belongs to.
    pub fn catalogue(&self) -> StyleCatalogue {
        self.body.catalogue()
    }

    /// Its title block, if it is a frame style holding one.
    pub fn title_block(&self) -> Option<&crate::objects::title_block::TitleBlockLayout> {
        match &self.body {
            StyleBody::Frame(f) => f.title_block.as_ref(),
            StyleBody::Cable(_)
            | StyleBody::Location(_)
            | StyleBody::Page(_)
            | StyleBody::RatingTable(_)
            | StyleBody::Text(_)
            | StyleBody::Wire(_) => None,
        }
    }
}

/// A style record's typed body (design 072 §8): one variant per catalogue, so a record can only
/// hold its own catalogue's fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StyleBody {
    Cable(CableType),
    Frame(FrameStyle),
    Location(LocationStyle),
    Page(PageType),
    #[serde(rename = "ratingtable")]
    RatingTable(RatingTable),
    Text(TextStyle),
    Wire(WireType),
}

impl StyleBody {
    /// A body of `catalogue` that sets nothing.
    pub fn empty(catalogue: StyleCatalogue) -> StyleBody {
        match catalogue {
            StyleCatalogue::Cable => StyleBody::Cable(CableType::default()),
            StyleCatalogue::Frame => StyleBody::Frame(FrameStyle::default()),
            StyleCatalogue::Location => StyleBody::Location(LocationStyle::default()),
            StyleCatalogue::Page => StyleBody::Page(PageType::default()),
            StyleCatalogue::RatingTable => StyleBody::RatingTable(RatingTable::default()),
            StyleCatalogue::Text => StyleBody::Text(TextStyle::default()),
            StyleCatalogue::Wire => StyleBody::Wire(WireType::default()),
        }
    }

    /// Which catalogue it belongs to.
    pub fn catalogue(&self) -> StyleCatalogue {
        match self {
            StyleBody::Cable(_) => StyleCatalogue::Cable,
            StyleBody::Frame(_) => StyleCatalogue::Frame,
            StyleBody::Location(_) => StyleCatalogue::Location,
            StyleBody::Page(_) => StyleCatalogue::Page,
            StyleBody::RatingTable(_) => StyleCatalogue::RatingTable,
            StyleBody::Text(_) => StyleCatalogue::Text,
            StyleBody::Wire(_) => StyleCatalogue::Wire,
        }
    }

    /// The fields it sets, as `(key, stored text)`: for messages and the inspector.
    pub fn entries(&self) -> Vec<(String, String)> {
        match self {
            StyleBody::Cable(c) => c.entries().into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            StyleBody::Page(p) => p.entries().into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            StyleBody::RatingTable(r) => r.entries().into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            StyleBody::Location(l) => l.entries().into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            StyleBody::Text(t) => t.entries().into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            StyleBody::Frame(f) => f.entries().into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            StyleBody::Wire(w) => w.entries().into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
        }
    }

    /// Whether it sets nothing.
    pub fn is_empty(&self) -> bool {
        match self {
            StyleBody::Cable(c) => *c == CableType::default(),
            StyleBody::Page(p) => *p == PageType::default(),
            StyleBody::RatingTable(r) => *r == RatingTable::default(),
            StyleBody::Location(l) => *l == LocationStyle::default(),
            StyleBody::Text(t) => *t == TextStyle::default(),
            StyleBody::Frame(f) => *f == FrameStyle::default(),
            StyleBody::Wire(w) => *w == WireType::default(),
        }
    }

    /// The stored text of field `key`, if the body sets it.
    pub fn entry(&self, key: &str) -> Option<String> {
        self.entries().into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Its cable type, if it is one.
    pub fn cable(&self) -> Option<&CableType> {
        match self {
            StyleBody::Cable(c) => Some(c),
            StyleBody::Frame(_)
            | StyleBody::Location(_)
            | StyleBody::Page(_)
            | StyleBody::RatingTable(_)
            | StyleBody::Text(_)
            | StyleBody::Wire(_) => None,
        }
    }

    /// Its frame style, if it is one.
    pub fn frame(&self) -> Option<&FrameStyle> {
        match self {
            StyleBody::Frame(f) => Some(f),
            StyleBody::Cable(_)
            | StyleBody::Location(_)
            | StyleBody::Page(_)
            | StyleBody::RatingTable(_)
            | StyleBody::Text(_)
            | StyleBody::Wire(_) => None,
        }
    }

    /// Its page type, if it is one.
    pub fn page(&self) -> Option<&PageType> {
        match self {
            StyleBody::Page(p) => Some(p),
            StyleBody::Cable(_)
            | StyleBody::Frame(_)
            | StyleBody::Location(_)
            | StyleBody::RatingTable(_)
            | StyleBody::Text(_)
            | StyleBody::Wire(_) => None,
        }
    }

    /// Its text style, if it is one.
    pub fn text(&self) -> Option<&TextStyle> {
        match self {
            StyleBody::Text(t) => Some(t),
            StyleBody::Cable(_)
            | StyleBody::Frame(_)
            | StyleBody::Location(_)
            | StyleBody::Page(_)
            | StyleBody::RatingTable(_)
            | StyleBody::Wire(_) => None,
        }
    }

    /// Its wire type, if it is one.
    pub fn wire(&self) -> Option<&WireType> {
        match self {
            StyleBody::Wire(w) => Some(w),
            StyleBody::Cable(_)
            | StyleBody::Frame(_)
            | StyleBody::Location(_)
            | StyleBody::Page(_)
            | StyleBody::RatingTable(_)
            | StyleBody::Text(_) => None,
        }
    }
}

/// A layer's style records: catalogue → style → record.
pub type StyleCatalogues = BTreeMap<StyleCatalogue, BTreeMap<StyleRef, StyleRecord>>;

/// Where a standard came from (reserved for 2.1, `.ecadlib`; always `None` in 050).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandardOrigin {
    pub library: LibraryId,
}

/// A standard's header (design 050 §2.3): changed as one unit by [`crate::Op::SetStandard`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandardHeader {
    /// Kept when a project copies the standard, so 2.1 can match it with a library's.
    pub id: StandardId,
    /// A label, 1–64 characters.
    pub name: StyleName,
    /// The built-in it starts from.
    pub based_on: BasedOn,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<StandardOrigin>,
}

crate::word_enum! {
    /// The built-in base a standard starts from (only IEC in 050).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum BasedOn {
        #[default]
        Iec,
    }
}

/// A project's copy of a standard (design 050 §2.3, FP1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Standard {
    pub header: StandardHeader,
    /// Registry values at the Standard level, by registry key (`tags.coil.prefix` → `K`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: Properties,
    /// Standard-layer style records.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub styles: StyleCatalogues,
}

impl Standard {
    /// A standard with `header`, no values and no styles.
    pub fn new(header: StandardHeader) -> Self {
        Standard { header, values: Properties::new(), styles: StyleCatalogues::new() }
    }
}

/// The record `layer` holds for `(catalogue, id)` in `doc`, if any.
pub fn record<'a>(doc: &'a crate::Document, layer: StyleLayer, catalogue: StyleCatalogue, id: &StyleRef) -> Option<&'a StyleRecord> {
    layer_of(doc, layer)?.get(&catalogue)?.get(id)
}

/// The record `layer` holds for `(catalogue, id)`, within [`MAX_STYLES_PER_CATALOGUE`]: past the
/// bound a record counts as absent (by ID order). The rank costs at most the bound.
pub fn bounded_record<'a>(doc: &'a crate::Document, layer: StyleLayer, catalogue: StyleCatalogue, id: &StyleRef) -> Option<&'a StyleRecord> {
    let map = layer_of(doc, layer)?.get(&catalogue)?;
    let r = map.get(id)?;
    if map.len() > MAX_STYLES_PER_CATALOGUE && map.range(..id).take(MAX_STYLES_PER_CATALOGUE).count() >= MAX_STYLES_PER_CATALOGUE {
        return None;
    }
    Some(r)
}

/// The style records of `layer` in `doc` (`None` for the standard layer without a standard).
pub fn layer_of(doc: &crate::Document, layer: StyleLayer) -> Option<&StyleCatalogues> {
    match layer {
        StyleLayer::Project => Some(&doc.styles),
        StyleLayer::Standard => doc.standard.as_ref().map(|s| &s.styles),
    }
}

/// Write `after` as the record of `(catalogue, id)` in `cats`, removing it (and an emptied
/// catalogue) for `None`. Returns the record it replaced.
pub(crate) fn put(cats: &mut StyleCatalogues, catalogue: StyleCatalogue, id: &StyleRef, after: Option<&StyleRecord>) -> Option<StyleRecord> {
    match after {
        Some(r) => cats.entry(catalogue).or_default().insert(id.clone(), r.clone()),
        None => {
            let inner = cats.get_mut(&catalogue)?;
            let old = inner.remove(id);
            if inner.is_empty() {
                cats.remove(&catalogue);
            }
            old
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_refs_parse_and_print() {
        assert_eq!(StyleRef::parse("tag"), Some(StyleRef::Builtin("tag".into())));
        assert_eq!(StyleRef::parse(" dc24 "), Some(StyleRef::Builtin("dc24".into())));
        let own = StyleRef::Own(0xab);
        assert_eq!(StyleRef::parse(&own.text()), Some(own.clone()));
        assert_eq!(own.text().len(), 33);
        for bad in ["", "Tag", "a-b", "#12", "#zz000000000000000000000000000000", &"x".repeat(33)] {
            assert_eq!(StyleRef::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn names_are_bounded() {
        assert!(is_name("Tag large"));
        assert!(!is_name(""));
        assert!(!is_name("  "));
        assert!(!is_name("a\u{7}b"));
        assert!(!is_name(&"n".repeat(65)));
    }
}
