//! The property keys a project may store, in one list (design 072 §9.2).
//!
//! After lanes B–F the property bags hold only presentation choices, the defaults registry's values
//! stored per object, and keys a person adds by hand. Every key the program itself reads or writes is
//! named here once:
//! - [`KEYS`]: a fixed key (`tag`, `number`), with its [`Area`], the objects that may hold it and the
//!   shape of its value. While lanes B–F move keys into struct fields, it also holds each key not yet
//!   moved; each lane takes its keys out and adds them to [`RESERVED`].
//! - [`FAMILIES`]: keys built from a pattern (`text.<kind>.style`, `link.<n>`), one
//!   [`Family::build`] / [`Family::parse`] pair for all of them.
//! - [`RESERVED`]: keys that became struct fields. A bag holding one is a read error (§9.4).
//!
//! The defaults registry's own entries (`text.tag.height`) are listed by the registry in
//! ecad-electrical, not here; [`area_of`] still classifies them by their first word.
//!
//! This module excludes what a key's value means: [`PropsExt`] reads a value's shape (yes/no,
//! whole number, text), and the module that owns a key decides what it does.
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;

use crate::doc::Properties;
use crate::words::{self, Word};

/// A property key the program reads or writes, named once in [`KEYS`] (or built by a
/// [`Family`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PropKey(pub &'static str);

impl PropKey {
    /// The key text, as stored.
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for PropKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

crate::word_enum! {
    /// What part of the project a key belongs to: the one place a key's prefix is read (design 064
    /// §3.3). Views route property writes by it.
    ///
    /// The first seventeen are the defaults registry's areas, in its order and with its words (a
    /// registry key starts with `<word>.`); the rest classify the stored keys outside the registry.
    /// Lanes B–F remove the transitional areas as their keys become struct fields, leaving the registry
    /// areas and [`Area::Free`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Area {
        Text,
        Wiring,
        Wire,
        Tags,
        Revisions,
        Print,
        Symbol,
        Terminal,
        Connector,
        #[strum(serialize = "strippage")]
        StripPage,
        Bom,
        Location,
        Continuation,
        Plc,
        Page,
        Front,
        Rating,
        /// A device's, wire's or cable's identity: its tag or wire number and whether it is fixed.
        Identity,
        /// Tag numbering's project state: provisional tags, high-water resets.
        Numbering,
        /// A page's place in the book: its number and folder.
        Book,
        /// The `=` function and `+` location codes stored on a page or the project.
        Designation,
        /// A generated page's kind.
        Generated,
        /// A terminal strip's jumpers (`link.<n>`).
        Link,
        /// Anything else: a person's own property.
        Free,
    }
}

impl Area {
    /// The defaults registry's areas, in its order (design 064 §3.3).
    pub const REGISTRY: [Area; 17] = [
        Area::Text,
        Area::Wiring,
        Area::Wire,
        Area::Tags,
        Area::Revisions,
        Area::Print,
        Area::Symbol,
        Area::Terminal,
        Area::Connector,
        Area::StripPage,
        Area::Bom,
        Area::Location,
        Area::Continuation,
        Area::Plc,
        Area::Page,
        Area::Front,
        Area::Rating,
    ];

    /// Whether this is one of the registry's areas.
    pub fn is_registry(self) -> bool {
        Area::REGISTRY.contains(&self)
    }
}

/// An object that may hold a property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Holder {
    Project,
    Page,
    Device,
    Wire,
    Cable,
    Representation,
}

/// The shape a key's stored value must have.
#[derive(Debug, Clone, Copy)]
pub enum ValueSpec {
    /// Free text of 1 to this many characters.
    Text { max_chars: usize },
    /// A yes/no value ([`words::flag`]); written `yes` or `no`.
    Flag,
    /// A whole number of at most `max` ([`words::uint`]).
    Uint { max: u32 },
    /// Text that the module owning the key reads with its own rule (a tag, a sheet spec, a link
    /// range).
    Ruled,
}

/// A fixed key and what may hold it.
#[derive(Debug, Clone, Copy)]
pub struct KeySpec {
    pub key: PropKey,
    pub area: Area,
    pub holders: &'static [Holder],
    pub value: ValueSpec,
}

/// Keys built from a pattern: each `{}` is one part (a word, an id, a number). Every part but a
/// last one at the end of the pattern is one dot-free segment; a last part at the very end may hold
/// dots.
#[derive(Debug, Clone, Copy)]
pub struct Family {
    /// `text.{}.style`.
    pub pattern: &'static str,
    pub area: Area,
    pub holders: &'static [Holder],
    pub value: ValueSpec,
}

impl Family {
    /// The number of parts the pattern takes.
    pub fn arity(&self) -> usize {
        self.pattern.matches("{}").count()
    }

    /// The key with these parts; `None` when the count is wrong, a part is empty, or a part that
    /// must be one segment holds a dot.
    pub fn build(&self, parts: &[&str]) -> Option<String> {
        let pieces: Vec<&str> = self.pattern.split("{}").collect();
        if parts.len() + 1 != pieces.len() {
            return None;
        }
        let mut out = String::with_capacity(self.pattern.len() + parts.iter().map(|p| p.len()).sum::<usize>());
        for (i, piece) in pieces.iter().enumerate() {
            out.push_str(piece);
            if let Some(part) = parts.get(i) {
                let open_end = i + 2 == pieces.len() && pieces.last().is_some_and(|p| p.is_empty());
                if part.is_empty() || (!open_end && part.contains('.')) {
                    return None;
                }
                out.push_str(part);
            }
        }
        Some(out)
    }

    /// The parts of `key` when it is one of this family's keys.
    pub fn parse<'k>(&self, key: &'k str) -> Option<Vec<&'k str>> {
        let pieces: Vec<&str> = self.pattern.split("{}").collect();
        let (first, rest) = pieces.split_first()?;
        let mut tail = key.strip_prefix(first)?;
        let mut parts = Vec::with_capacity(rest.len());
        for (i, piece) in rest.iter().enumerate() {
            let last = i + 1 == rest.len();
            let part = if last && piece.is_empty() {
                std::mem::take(&mut tail)
            } else {
                let end = tail.find('.').unwrap_or(tail.len());
                let (part, after) = tail.split_at(end);
                tail = after.strip_prefix(piece)?;
                part
            };
            if part.is_empty() {
                return None;
            }
            parts.push(part);
        }
        tail.is_empty().then_some(parts)
    }
}

/// A device's description (a cable's until lane D).
pub const DESCRIPTION: PropKey = PropKey(crate::device::DESCRIPTION_KEY);
/// A device's part number.
pub const PART_NUMBER: PropKey = PropKey(crate::device::PART_NUMBER_KEY);
/// A device's maker.
pub const MAKER: PropKey = PropKey(crate::device::MAKER_KEY);
/// A device's article number.
pub const ARTICLE_NUMBER: PropKey = PropKey(crate::device::ARTICLE_KEY);
/// A device's type text (a cable's until lane D).
pub const TYPE_TEXT: PropKey = PropKey(crate::device::TYPE_KEY);
/// A device's or cable's tag.
pub const TAG: PropKey = PropKey(crate::tag::TAG_KEY);
/// Whether a device's or cable's tag is fixed (yes/no).
pub const TAG_FIXED: PropKey = PropKey(crate::tag::FIXED_KEY);
/// Whether the project gives provisional tags (yes/no).
pub const NUMBERING_PROVISIONAL: PropKey = PropKey(crate::tag::PROVISIONAL_SETTING);
/// A wire's number.
pub const WIRE_NUMBER: PropKey = PropKey(crate::connectivity::WIRE_NUMBER);
/// Whether a wire's number is fixed (yes/no).
pub const WIRE_NUMBER_FIXED: PropKey = PropKey(crate::connectivity::WIRE_NUMBER_FIXED);
/// A page's number.
pub const NUMBER: PropKey = PropKey(crate::book::NUMBER_KEY);
/// A page's folder.
pub const FOLDER: PropKey = PropKey(crate::book::FOLDER_KEY);
/// The `=` function code of a page or the project.
pub const FUNCTION: PropKey = PropKey(crate::book::FUNCTION_KEY);
/// The `+` location code of a page or the project.
pub const LOCATION: PropKey = PropKey(crate::book::LOCATION_KEY);
/// A page's sheet.
pub const SHEET: PropKey = PropKey(crate::sheet::SHEET_KEY);
/// A page's title block.
pub const TITLE_BLOCK: PropKey = PropKey(crate::sheet::TITLE_BLOCK_KEY);
/// A generated page's kind.
pub const GENERATED: PropKey = PropKey(crate::sheet::GENERATED_KEY);

/// The fixed keys the program stores outside the defaults registry. One key per line; lanes add or
/// remove lines.
#[rustfmt::skip]
pub const KEYS: &[KeySpec] = &[
    KeySpec { key: TAG, area: Area::Identity, holders: &[Holder::Wire, Holder::Cable], value: ValueSpec::Ruled },
    KeySpec { key: TAG_FIXED, area: Area::Identity, holders: &[Holder::Cable], value: ValueSpec::Flag },
    KeySpec { key: WIRE_NUMBER, area: Area::Identity, holders: &[Holder::Wire], value: ValueSpec::Ruled },
    KeySpec { key: WIRE_NUMBER_FIXED, area: Area::Identity, holders: &[Holder::Wire], value: ValueSpec::Flag },
    KeySpec { key: NUMBERING_PROVISIONAL, area: Area::Numbering, holders: &[Holder::Project], value: ValueSpec::Flag },
    KeySpec { key: NUMBER, area: Area::Book, holders: &[Holder::Page], value: ValueSpec::Ruled },
    KeySpec { key: FOLDER, area: Area::Book, holders: &[Holder::Page], value: ValueSpec::Ruled },
    KeySpec { key: FUNCTION, area: Area::Designation, holders: &[Holder::Page, Holder::Project, Holder::Device], value: ValueSpec::Ruled },
    KeySpec { key: LOCATION, area: Area::Designation, holders: &[Holder::Page, Holder::Project, Holder::Device], value: ValueSpec::Ruled },
    KeySpec { key: SHEET, area: Area::Page, holders: &[Holder::Page], value: ValueSpec::Ruled },
    KeySpec { key: TITLE_BLOCK, area: Area::Page, holders: &[Holder::Page], value: ValueSpec::Ruled },
    KeySpec { key: GENERATED, area: Area::Generated, holders: &[Holder::Page], value: ValueSpec::Ruled },
];

/// The key families. One family per line.
#[rustfmt::skip]
pub const FAMILIES: &[Family] = &[
    Family { pattern: "text.{}.{}.style", area: Area::Text, holders: &[Holder::Project, Holder::Page, Holder::Device, Holder::Representation], value: ValueSpec::Ruled },
    Family { pattern: "text.{}.{}.show", area: Area::Text, holders: &[Holder::Project, Holder::Page, Holder::Device, Holder::Representation], value: ValueSpec::Ruled },
    Family { pattern: "link.{}", area: Area::Link, holders: &[Holder::Device], value: ValueSpec::Ruled },
    Family { pattern: "numbering.high.{}", area: Area::Numbering, holders: &[Holder::Project], value: ValueSpec::Ruled },
];

/// Keys that became struct fields: a property bag holding one is a read error (design 072 §9.4).
/// Lanes B–F move keys here from [`KEYS`].
pub const RESERVED: &[PropKey] = &[];

/// The fixed key `key`, if it is one.
pub fn spec(key: &str) -> Option<&'static KeySpec> {
    KEYS.iter().find(|s| s.key.0 == key)
}

/// The family `key` belongs to, if any.
pub fn family_of(key: &str) -> Option<&'static Family> {
    FAMILIES.iter().find(|f| f.parse(key).is_some())
}

/// Whether `key` became a struct field ([`RESERVED`]).
pub fn is_reserved(key: &str) -> bool {
    RESERVED.iter().any(|k| k.0 == key)
}

/// Keys that became fields of [`crate::Device`] (design 072 §2.1) while other holders still keep
/// them in their bags (a cable's `tag` until lane D): a device's bag holding one is a read error,
/// and `SetProperty` refuses them on a device.
pub const DEVICE_FIELDS: &[PropKey] = &[TAG, TAG_FIXED, DESCRIPTION, PART_NUMBER, MAKER, ARTICLE_NUMBER, TYPE_TEXT];

/// Whether `key` became a struct field on `holder` ([`RESERVED`], [`DEVICE_FIELDS`]).
pub fn is_reserved_on(key: &str, holder: Holder) -> bool {
    is_reserved(key) || (holder == Holder::Device && DEVICE_FIELDS.iter().any(|k| k.0 == key))
}

/// The area `key` belongs to.
///
/// # Semantics
/// A fixed key's or a family's own area; else the registry area its first word names (`print.…`
/// is [`Area::Print`]); else [`Area::Free`].
pub fn area_of(key: &str) -> Area {
    if let Some(s) = spec(key) {
        return s.area;
    }
    if let Some(f) = family_of(key) {
        return f.area;
    }
    let first = key.split('.').next().unwrap_or(key);
    match Area::REGISTRY.iter().find(|a| a.word() == first) {
        Some(a) if key.len() > first.len() => *a,
        Some(_) | None => Area::Free,
    }
}

/// Typed readers on a property bag. A value that does not have the shape asked for reads as
/// `None`, like a missing one.
pub trait PropsExt {
    /// The text stored under `key`.
    fn text(&self, key: PropKey) -> Option<&str>;

    /// The yes/no value stored under `key` ([`words::flag`]).
    fn flag(&self, key: PropKey) -> Option<bool> {
        self.text(key).and_then(words::flag)
    }

    /// The whole number of at most `max` stored under `key` ([`words::uint`]).
    fn uint(&self, key: PropKey, max: u32) -> Option<u32> {
        self.text(key).and_then(|t| words::uint(t, max))
    }

    /// The word of `E` stored under `key` ([`Word::parse`]).
    fn word<E: Word>(&self, key: PropKey) -> Option<E> {
        self.text(key).and_then(E::parse)
    }
}

impl PropsExt for Properties {
    fn text(&self, key: PropKey) -> Option<&str> {
        self.get(key.0).map(String::as_str)
    }
}
