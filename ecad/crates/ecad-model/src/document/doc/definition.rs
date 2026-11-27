//! Symbol definitions: text slots, defaults, connection point places.

use serde::{Deserialize, Serialize};

use crate::geometry::geom::Pt;
use crate::geometry::point::PointKey;
use crate::geometry::units::Radians;
use crate::naming::text::Text;

use super::{device::*, drawing::*, settings::*};
use crate::objects::devices::DeviceType;
use crate::objects::role::PartRole;

/// The device text a symbol slot places: one of the device's own
/// texts, or the rating of one of its connection points (`rating.A1`).
///
/// # Semantics
/// Stored as its word (`tag`, `part_number`, `rating.A1`), so a file reads as it always did. A
/// point's label is 1 to 64 characters with no control character.
///
/// # Example
/// ```
/// use ecad_model::SlotText;
/// assert_eq!(SlotText::parse("rating.A1"), Some(SlotText::PointRating("A1".into())));
/// assert_eq!(SlotText::PartNumber.word(), "part_number");
/// assert_eq!(SlotText::parse("colour"), None);
/// assert_eq!(serde_json::to_string(&SlotText::Tag).unwrap(), r#""tag""#);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum SlotText {
    Tag,
    Description,
    Rating,
    PartNumber,
    Maker,
    ArticleNumber,
    Function,
    Location,
    /// The rating of the connection point labelled so.
    PointRating(String),
}

impl SlotText {
    /// The device's own texts, in the order a symbol lists them.
    pub const OWN: [SlotText; 8] = [
        SlotText::Tag,
        SlotText::Description,
        SlotText::Rating,
        SlotText::PartNumber,
        SlotText::Maker,
        SlotText::ArticleNumber,
        SlotText::Function,
        SlotText::Location,
    ];

    /// Most characters of a point label in [`SlotText::PointRating`].
    pub(crate) const MAX_LABEL: usize = 64;

    /// Its word, as stored and typed (`part_number`, `rating.A1`).
    pub fn word(&self) -> std::borrow::Cow<'static, str> {
        use std::borrow::Cow;
        Cow::Borrowed(match self {
            SlotText::Tag => "tag",
            SlotText::Description => "description",
            SlotText::Rating => "rating",
            SlotText::PartNumber => "part_number",
            SlotText::Maker => "maker",
            SlotText::ArticleNumber => "article_number",
            SlotText::Function => "function",
            SlotText::Location => "location",
            SlotText::PointRating(l) => return Cow::Owned(format!("rating.{l}")),
        })
    }

    /// The slot text a word names, if it names one.
    pub fn parse(word: &str) -> Option<SlotText> {
        if let Some(l) = word.strip_prefix("rating.") {
            let ok = !l.is_empty() && l.chars().count() <= Self::MAX_LABEL && !l.chars().any(char::is_control);
            return ok.then(|| SlotText::PointRating(l.to_string()));
        }
        SlotText::OWN.into_iter().find(|t| t.word() == word)
    }
}

impl std::fmt::Display for SlotText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.word())
    }
}

/// A word that names no device text a slot may place.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a device text")]
pub struct NotASlotText(pub String);

impl TryFrom<String> for SlotText {
    type Error = NotASlotText;
    fn try_from(s: String) -> std::result::Result<SlotText, NotASlotText> {
        SlotText::parse(&s).ok_or(NotASlotText(s))
    }
}

impl From<SlotText> for String {
    fn from(t: SlotText) -> String {
        t.word().into_owned()
    }
}

/// Where a device text of a placed copy goes, in the symbol's frame (a definition's text slots). An anchor
/// only: size and style come from the text styles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextSlot {
    /// The device text it places.
    pub text: SlotText,
    pub at: Pt,
    #[serde(default, skip_serializing_if = "is_default")]
    pub h: TextHAlign,
    #[serde(default, skip_serializing_if = "is_default")]
    pub v: TextVAlign,
    /// Counter-clockwise.
    #[serde(default, skip_serializing_if = "is_default")]
    pub rotation: Radians,
}

/// How a placed copy of a symbol joins a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Join {
    /// A device of its own: a new tag on each placement.
    #[default]
    Device,
    /// Part of a named device.
    Part,
}

impl Join {
    /// The word a person types and sees: `device` or `part`.
    pub fn word(self) -> &'static str {
        match self {
            Join::Device => "device",
            Join::Part => "part",
        }
    }

    /// The join a word names, ignoring case and spaces around it.
    pub fn of_word(w: &str) -> Option<Join> {
        [Join::Device, Join::Part].into_iter().find(|j| j.word().eq_ignore_ascii_case(w.trim()))
    }
}

/// Why a symbol default was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SymbolDefaultError {
    /// A `join` that is neither `device` nor `part`.
    #[error("A symbol joins a device as `device` or `part`.")]
    Join,
    /// A text value that is too long or holds a hidden character.
    #[error("The symbol's {which}: {error}.")]
    Text { which: &'static str, error: crate::naming::text::TextError },
}

/// One of a symbol's own default values, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SymbolDefault {
    Key,
    Prefix,
    Description,
    Rating,
    Join,
    Aliases,
    Family,
}

impl SymbolDefault {
    /// Every default, in the order a symbol lists them.
    pub const ALL: [SymbolDefault; 7] = [
        SymbolDefault::Key,
        SymbolDefault::Prefix,
        SymbolDefault::Description,
        SymbolDefault::Rating,
        SymbolDefault::Join,
        SymbolDefault::Aliases,
        SymbolDefault::Family,
    ];

    /// Its name in a command's `defaults` (`prefix`).
    pub fn word(self) -> &'static str {
        match self {
            SymbolDefault::Key => "key",
            SymbolDefault::Prefix => "prefix",
            SymbolDefault::Description => "description",
            SymbolDefault::Rating => "rating",
            SymbolDefault::Join => "join",
            SymbolDefault::Aliases => "aliases",
            SymbolDefault::Family => "family",
        }
    }

    /// The default a name names, exactly.
    pub fn of_word(w: &str) -> Option<SymbolDefault> {
        SymbolDefault::ALL.into_iter().find(|d| d.word() == w)
    }
}

/// A symbol's own default values: typed fields, each left out of
/// the file when not set. The starters have none.
///
/// # Semantics
/// - `key` names a project symbol; `prefix` is a new device's tag prefix; `description` and
///   `rating` are a new device's texts (the rating read against the definition's device type when
///   the symbol is saved); `join` says whether a placed copy is a device of its own or a part of a
///   named one; `aliases` are search words; `family` groups symbols in the library.
/// - Text values are kept trimmed and never empty: [`SymbolDefaults::set`] clears a value given as
///   blank. Lengths and shapes are the symbol command's rules.
///
/// # Example
/// ```
/// use ecad_model::{Join, SymbolDefault, SymbolDefaults};
/// let mut d = SymbolDefaults::default();
/// d.set(SymbolDefault::Key, " float1 ").unwrap();
/// d.set(SymbolDefault::Join, "Part").unwrap();
/// d.set(SymbolDefault::Aliases, "level, float").unwrap();
/// assert_eq!(d.key.as_ref().map(|k| k.as_str()), Some("float1"));
/// assert_eq!(d.join, Some(Join::Part));
/// assert_eq!(d.get(SymbolDefault::Aliases).as_deref(), Some("level, float"));
/// assert_eq!(
///     serde_json::to_string(&d).unwrap(),
///     r#"{"key":"float1","join":"part","aliases":["level","float"]}"#
/// );
/// assert!(d.set(SymbolDefault::Join, "both").is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolDefaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<Text<64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<Text<64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Text<400>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<Text<200>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<Join>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<Text<200>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<Text<64>>,
}

impl SymbolDefaults {
    /// Whether no default is set.
    pub fn is_empty(&self) -> bool {
        *self == SymbolDefaults::default()
    }

    /// Default `which` as text (`join` as its word, `aliases` joined by `, `), if set.
    pub fn get(&self, which: SymbolDefault) -> Option<String> {
        match which {
            SymbolDefault::Key => self.key.as_ref().map(Text::to_string),
            SymbolDefault::Prefix => self.prefix.as_ref().map(Text::to_string),
            SymbolDefault::Description => self.description.as_ref().map(Text::to_string),
            SymbolDefault::Rating => self.rating.as_ref().map(Text::to_string),
            SymbolDefault::Join => self.join.map(|j| j.word().to_string()),
            SymbolDefault::Aliases => (!self.aliases.is_empty()).then(|| self.aliases.iter().map(Text::as_str).collect::<Vec<_>>().join(", ")),
            SymbolDefault::Family => self.family.as_ref().map(Text::to_string),
        }
    }

    /// Sets default `which` from typed text, trimmed; blank text clears it. `aliases` is a list
    /// separated by commas.
    ///
    /// # Errors
    /// [`SymbolDefaultError`]: a `join` that is neither `device` nor `part`, or a text that is too
    /// long or holds a hidden character.
    pub fn set(&mut self, which: SymbolDefault, value: &str) -> std::result::Result<(), SymbolDefaultError> {
        fn text<const N: usize>(which: SymbolDefault, v: &str) -> std::result::Result<Option<Text<N>>, SymbolDefaultError> {
            if v.is_empty() {
                return Ok(None);
            }
            Text::new(v).map(Some).map_err(|error| SymbolDefaultError::Text { which: which.word(), error })
        }
        let v = value.trim();
        match which {
            SymbolDefault::Key => self.key = text(which, v)?,
            SymbolDefault::Prefix => self.prefix = text(which, v)?,
            SymbolDefault::Description => self.description = text(which, v)?,
            SymbolDefault::Rating => self.rating = text(which, v)?,
            SymbolDefault::Join => {
                self.join = if v.is_empty() { None } else { Some(Join::of_word(v).ok_or(SymbolDefaultError::Join)?) };
            }
            SymbolDefault::Aliases => {
                self.aliases = v
                    .split(',')
                    .map(str::trim)
                    .filter(|a| !a.is_empty())
                    .map(|a| Text::new(a).map_err(|error| SymbolDefaultError::Text { which: which.word(), error }))
                    .collect::<std::result::Result<_, _>>()?;
            }
            SymbolDefault::Family => self.family = text(which, v)?,
        }
        Ok(())
    }
}

/// A project-local copy of a library definition.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    pub origin: Option<Origin>,
    pub graphics: Vec<Primitive>,
    pub connection_points: Vec<ConnectionPointDef>,
    /// Groups of terminal keys joined inside the device: every terminal of one
    /// group is on one net. Empty for almost every definition, and then left out of the file so
    /// existing content hashes do not change. A bridge may name a key the placement writes rather
    /// than one of the definition's own (a connector half bridges the side keys
    /// `.s` and `.p`; [`crate::document::invariant::is_placement_key`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bridges: Vec<Vec<PointKey>>,
    /// Where the device texts of a placed copy go. Empty for the starters, whose
    /// anchors are catalogue code; left out of the file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<TextSlot>,
    /// The symbol's own default values. Empty for the starters;
    /// left out of the file when empty.
    #[serde(default, skip_serializing_if = "SymbolDefaults::is_empty")]
    pub defaults: SymbolDefaults,
    /// The origin of the definition an edited copy came from. Never followed by
    /// the reader, so a cycle cannot loop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<Origin>,
    /// What a device this symbol starts is: placing the symbol as a new
    /// device copies it. Left out of the file for [`DeviceType::General`].
    #[serde(default, skip_serializing_if = "DeviceType::is_general")]
    pub device_type: DeviceType,
    /// A placed copy's part in its device: copied into the
    /// representation's [`Role::Part`](crate::objects::role::Role::Part) when the symbol is placed. Left out of the file for
    /// [`PartRole::Other`].
    #[serde(default, skip_serializing_if = "PartRole::is_other")]
    pub part_role: PartRole,
}

impl Definition {
    /// The keys bridged to `key` inside the device (not `key` itself), from [`Definition::bridges`].
    pub(crate) fn bridged_with<'a>(&'a self, key: &'a PointKey) -> impl Iterator<Item = &'a PointKey> + 'a {
        self.bridges.iter().filter(move |g| g.contains(key)).flat_map(|g| g.iter()).filter(move |k| *k != key)
    }
}
