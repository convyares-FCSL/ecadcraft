//! Project settings and the content hash.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::naming::id::{DefinitionId, LibraryId};

/// SHA-256 of a definition's canonical form (proposal §20.10), as 64 lower-case hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    /// The 64-hex-digit form.
    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Parse the 64-hex-digit form.
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.as_bytes();
        if t.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, slot) in out.iter_mut().enumerate() {
            let pair = std::str::from_utf8(t.get(2 * i..2 * i + 2)?).ok()?;
            *slot = u8::from_str_radix(pair, 16).ok()?;
        }
        Some(Self(out))
    }
}

impl Serialize for ContentHash {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("content hash must be 64 hex digits"))
    }
}

/// Where an embedded definition came from (LIB-01, proposal §20.9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Origin {
    /// The library's stable identity.
    pub library: LibraryId,
    /// The definition's ID inside that library.
    pub definition: DefinitionId,
    /// The library transaction that last changed the definition (its version).
    pub library_seq: u64,
    /// Hash of the definition content as embedded.
    pub content_hash: ContentHash,
}

/// Units of lengths and coordinates in the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LengthUnit {
    #[default]
    Millimetre,
    Inch,
}

/// Project-wide settings (proposal §3.2, §20.13). Set at creation; changed only by a recorded op.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectSettings {
    pub length_unit: LengthUnit,
    /// How many digits automatic tag numbers are padded to (brief 018), when the project chose
    /// (brief 066, F3: `None` follows the built-in, [`ProjectSettings::tag_padding`]). Values
    /// above [`crate::naming::tag::MAX_PADDING`] count as that maximum.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_padding: Option<u8>,
    /// The project's own tag prefix per symbol (`starter:coil → KA`, `project:valve-x → YV`),
    /// overriding a starter's built-in IEC 81346 default or a project symbol's own (design 072 F5).
    /// Empty by default.
    pub tag_prefixes: BTreeMap<crate::naming::prefix::PrefixOf, crate::naming::prefix::TagPrefix>,
    /// The prefix of new cable tags, when the project chose one (brief 026; design 072 F5: no
    /// longer a `tag_prefixes` entry). `None` follows the built-in `W`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cable_prefix: Option<crate::naming::prefix::TagPrefix>,
    /// How wires are numbered and what each potential defaults to (brief 022, FP1). Left out of
    /// the file while it holds [`WiringSettings::default`].
    pub wiring: crate::connectivity::wiring::WiringSettings,
    /// The project's sheet: size and grid of every page's frame unless a page sets its own
    /// (brief 019, decision FP2), when the project chose one (brief 066, F3: `None` follows the
    /// built-in, [`ProjectSettings::sheet`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet: Option<crate::document::sheet::SheetSpec>,
    /// What a new page's number adds to the number before it (design 019 §6.1), when the project
    /// chose (brief 066, F3: [`ProjectSettings::page_number_step`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_number_step: Option<u32>,
    /// How cross-references name a place (owner answer Q1, 2026-11-08), when the project chose
    /// (brief 066, F3: [`ProjectSettings::reference_style`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_style: Option<ReferenceStyle>,
    /// Where a device's contact mirror table goes unless the device says otherwise (brief 025,
    /// FP3): `auto` by default, and then left out of every file.
    #[serde(skip_serializing_if = "ContactMirror::is_auto")]
    pub contact_mirror: ContactMirror,
}

/// Where the contact mirror table under a coil goes by default (brief 025, FP3). A device's own
/// `mirror` property overrides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactMirror {
    /// Under the rail the coil's lower wire meets, else under the coil.
    #[default]
    Auto,
    /// Under the coil.
    Below,
    /// Right of the coil.
    Right,
    /// No tables (texts under contacts remain).
    Off,
}

impl ContactMirror {
    /// Whether it is the default (`auto`), which files leave out.
    pub fn is_auto(&self) -> bool {
        *self == ContactMirror::Auto
    }

    /// Its word: `auto`, `below`, `right` or `off`.
    pub fn word(self) -> &'static str {
        match self {
            ContactMirror::Auto => "auto",
            ContactMirror::Below => "below",
            ContactMirror::Right => "right",
            ContactMirror::Off => "off",
        }
    }

    /// The placement a word names (case-insensitive), if any.
    pub fn parse(word: &str) -> Option<ContactMirror> {
        [ContactMirror::Auto, ContactMirror::Below, ContactMirror::Right, ContactMirror::Off]
            .into_iter()
            .find(|m| m.word().eq_ignore_ascii_case(word.trim()))
    }
}

/// How a cross-reference names a page and a place on it (owner answer Q1, 2026-11-08; UI round 2
/// and rev 3: `2/B5`). The zone's letter comes first whichever axis is lettered
/// ([`crate::connectivity::derive::place_text`]). Other styles (US) come later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ReferenceStyle {
    /// `105-B-3`: page number, then the zone, letter first.
    PageColumnRow,
    /// `/105.3`: page number and column.
    PageColumn,
    /// `105/B3`: page number, then the zone, letter first (UI round 2 and rev 3; the default of a
    /// new project).
    #[default]
    PageZone,
}

impl ReferenceStyle {
    /// The words of the styles, as `projectprops` and the defaults data write them.
    pub const WORDS: &'static [&'static str] = &["page-zone", "page-column-row", "page-column"];

    /// Its word.
    pub fn word(self) -> &'static str {
        match self {
            ReferenceStyle::PageZone => "page-zone",
            ReferenceStyle::PageColumnRow => "page-column-row",
            ReferenceStyle::PageColumn => "page-column",
        }
    }

    /// The style a word names, if any.
    pub fn parse(word: &str) -> Option<ReferenceStyle> {
        [ReferenceStyle::PageZone, ReferenceStyle::PageColumnRow, ReferenceStyle::PageColumn].into_iter().find(|s| s.word() == word.trim())
    }
}

impl ProjectSettings {
    /// The project's own prefix for starter `word` (design 072 F5).
    pub fn starter_prefix(&self, word: &str) -> Option<&crate::naming::prefix::TagPrefix> {
        self.tag_prefixes.get(&crate::naming::prefix::PrefixOf::Starter(word.into()))
    }

    /// The project's own prefix for project symbol `key` (design 072 F5).
    pub fn project_prefix(&self, key: &str) -> Option<&crate::naming::prefix::TagPrefix> {
        self.tag_prefixes.get(&crate::naming::prefix::PrefixOf::Project(key.into()))
    }

    /// The tag padding: the project's choice, else the built-in ([`crate::naming::builtins`]).
    pub fn tag_padding(&self) -> u8 {
        self.tag_padding.unwrap_or_else(|| crate::naming::builtins::builtins().tag_padding)
    }

    /// The project sheet: the project's choice when it is within the limits, else the built-in.
    pub fn sheet(&self) -> crate::document::sheet::SheetSpec {
        self.sheet.filter(|s| s.check().is_ok()).unwrap_or(crate::naming::builtins::builtins().sheet)
    }

    /// The page number step, at least 1: the project's choice, else the built-in.
    pub fn page_number_step(&self) -> u32 {
        self.page_number_step.unwrap_or_else(|| crate::naming::builtins::builtins().page_number_step).max(1)
    }

    /// The reference style: the project's choice, else the built-in.
    pub fn reference_style(&self) -> ReferenceStyle {
        self.reference_style.unwrap_or_else(|| crate::naming::builtins::builtins().reference_style)
    }
}
