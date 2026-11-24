//! Devices and their connection points.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::geometry::geom::Pt;
use crate::geometry::point::PointKey;
use crate::naming::id::{DefinitionId, DeviceId};

use super::{Properties, definition::*, drawing::*};
use crate::objects::devices::DeviceType;

/// A device: a real part with its symbols, its connection points and its type.
///
/// # Semantics
/// What the device is lives in [`Device::device_type`], copied from its symbol's
/// [`Definition::device_type`] when the symbol is placed as a new device; it is never worked out
/// from the symbol, the tag or a property. The fields not yet typed
/// are still in [`Device::properties`].
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    /// Its tag (`-K01`) and what it is (the identity of the device): automatic, fixed
    /// (typed, kept by renumbering) or provisional, or none. One field: the state holds the tag.
    pub tag: crate::naming::tag::TagSlot,
    /// What it is for, in words (`setprop` key `description`).
    pub description: Option<crate::objects::devices::Description>,
    /// Its main part's number (`part_number`).
    pub part_number: Option<crate::objects::devices::PartText>,
    /// Its main part's maker (`maker`).
    pub manufacturer: Option<crate::objects::devices::PartText>,
    /// Its main part's article number (`article_number`).
    pub article_number: Option<crate::objects::devices::PartText>,
    /// Its type text (`type`): a terminal strip's type notation reads it.
    pub type_text: Option<crate::objects::devices::PartText>,
    /// Its location (an entry of the project's list) and how it came to be: manual
    /// (typed, or a box drawn around it: kept when a box around it changes), proposed (from its
    /// page or a box), or unset. One field: the state holds the entry.
    pub location: crate::document::book::Place<crate::naming::id::LocationId>,
    /// Its function (an entry of the project's list), as [`Device::location`].
    pub function: crate::document::book::Place<crate::naming::id::FunctionId>,
    /// Its rating note: text that did not read as its group's typed rating.
    pub rating_note: Option<crate::objects::devices::RatingNote>,
    pub properties: Properties,
    /// The embedded definition its symbol representations use, if any.
    pub definition: Option<DefinitionId>,
    /// What the device is.
    pub device_type: DeviceType,
    /// Its values of the project's user fields, by field; left out while empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub user: BTreeMap<crate::naming::id::UserFieldId, crate::objects::user::UserValue>,
}

impl Device {
    /// A new device drawn by `definition` (`id`), with the type the symbol declares.
    pub fn of_definition(id: DefinitionId, definition: &Definition, properties: Properties) -> Device {
        Device { properties, definition: Some(id), device_type: definition.device_type.clone(), ..Device::default() }
    }

    /// Whether its tag is fixed.
    pub fn is_fixed(&self) -> bool {
        self.tag.is_fixed()
    }

    /// Whether its tag is provisional.
    pub fn is_provisional(&self) -> bool {
        self.tag.is_provisional()
    }

    /// Its tag as drawn: a provisional tag with its `?` ([`crate::naming::tag::shown`]).
    pub fn tag_shown(&self) -> Option<String> {
        self.tag.shown()
    }

    /// Its tag's identity in the tag index.
    pub fn tag_key(&self) -> Option<crate::naming::tag::Key> {
        self.tag.key()
    }

    /// Whether `other` differs in a typed field (anything but the property bag and the
    /// definition, which have ops of their own).
    pub(crate) fn fields_differ(&self, other: &Device) -> bool {
        let typed = |d: &Device| Device { properties: Properties::new(), definition: None, ..d.clone() };
        typed(self) != typed(other)
    }

    /// Whether `other` differs from this device in its text fields only ([`crate::TextField`]):
    /// what a view that reads none of them (rating, connectivity) can skip.
    pub fn only_text_differs(&self, other: &Device) -> bool {
        let bare =
            |d: &Device| Device { description: None, part_number: None, manufacturer: None, article_number: None, type_text: None, ..d.clone() };
        bare(self) == bare(other)
    }

    /// Whether `other` differs from this device in its rating only (its note, and the typed rating
    /// fields of the same kind of device): what a view that reads a device's rating, not its
    /// points, scopes to the device alone.
    pub fn only_rating_differs(&self, other: &Device) -> bool {
        let bare = |d: &Device| Device { rating_note: None, device_type: d.device_type.unrated(), ..d.clone() };
        bare(self) == bare(other)
    }

    /// Whether `other` differs from this device in its location and function (and how they came to
    /// be) only: what a view that reads neither can skip.
    pub fn only_place_differs(&self, other: &Device) -> bool {
        let bare = |d: &Device| Device { location: Default::default(), function: Default::default(), ..d.clone() };
        bare(self) == bare(other)
    }

    /// The value `setprop`'s key `key` names on this device (the key stays a
    /// string and routes to the field): a typed field as its stored text, else the property.
    pub fn value(&self, key: &str) -> Option<String> {
        match key {
            crate::document::book::LOCATION_KEY => self.location.id().map(crate::naming::id::LocationId::hex),
            crate::document::book::FUNCTION_KEY => self.function.id().map(crate::naming::id::FunctionId::hex),
            _ if crate::objects::devices::Connector::is_key(key) => match &self.device_type {
                DeviceType::Connector(x) => x.value(key),
                _ => None,
            },
            crate::objects::devices::RATING_KEY => self.rating_shown(),
            _ => match (&self.device_type, key.strip_prefix(crate::objects::devices::RATING_OF_PREFIX)) {
                (DeviceType::Protection(crate::objects::devices::Protection::FuseBank(b)), Some(n)) => {
                    let n = n.parse::<u8>().ok()?;
                    b.outputs.get(&n).map(ToString::to_string).filter(|t| !t.is_empty())
                }
                (DeviceType::TerminalStrip(s), Some(p)) => s.fuse_rating(p),
                _ => self.get(key).map(str::to_string),
            },
        }
    }

    /// Its rating as shown: its typed rating fields when one is set, else its
    /// note. Nothing stores this text.
    pub fn rating_shown(&self) -> Option<String> {
        self.device_type.rating().or_else(|| self.rating_note.as_ref().map(ToString::to_string))
    }

    /// Whether its group has typed rating fields and none is set (the Checks row "no rating",
    /// whatever its note says).
    pub fn has_no_rating(&self) -> bool {
        self.device_type.has_rating_fields() && self.device_type.rating().is_none()
    }

    /// The text `setprop`'s key `key` names on this device: its tag, a text field
    /// ([`crate::TextField`]), else the property. (The tag's fixed flag is not text: [`Device::value`].)
    pub fn get(&self, key: &str) -> Option<&str> {
        if key == crate::naming::tag::TAG_KEY {
            return self.tag();
        }
        match crate::TextField::of_key(key) {
            Some(f) => self.text(f),
            None => self.properties.get(key).map(String::as_str),
        }
    }

    /// Its text field `f`.
    pub fn text(&self, f: crate::TextField) -> Option<&str> {
        use crate::TextField as F;
        match f {
            F::Description => self.description.as_ref().map(|t| t.as_str()),
            F::PartNumber => self.part_number.as_ref().map(|t| t.as_str()),
            F::Manufacturer => self.manufacturer.as_ref().map(|t| t.as_str()),
            F::ArticleNumber => self.article_number.as_ref().map(|t| t.as_str()),
            F::TypeText => self.type_text.as_ref().map(|t| t.as_str()),
        }
    }

    /// Set its text field `f` to `value` (`None`, or text that is blank once trimmed, clears it).
    ///
    /// # Errors
    /// The [`crate::naming::text::TextError`] of a value that is too long or holds a hidden character.
    pub fn set_text(&mut self, f: crate::TextField, value: Option<&str>) -> std::result::Result<(), crate::naming::text::TextError> {
        use crate::TextField as F;
        use crate::objects::devices::{Description, PartText};
        let value = value.map(str::trim).filter(|v| !v.is_empty());
        let part = |v: Option<&str>| v.map(PartText::new).transpose();
        match f {
            F::Description => self.description = value.map(Description::new).transpose()?,
            F::PartNumber => self.part_number = part(value)?,
            F::Manufacturer => self.manufacturer = part(value)?,
            F::ArticleNumber => self.article_number = part(value)?,
            F::TypeText => self.type_text = part(value)?,
        }
        Ok(())
    }

    /// This device with text field `f` set to `value` ([`Device::set_text`]).
    ///
    /// # Errors
    /// As [`Device::set_text`].
    pub fn with_text(&self, f: crate::TextField, value: Option<&str>) -> std::result::Result<Device, crate::naming::text::TextError> {
        let mut d = self.clone();
        d.set_text(f, value)?;
        Ok(d)
    }

    /// Set the value `setprop`'s key `key` names: a text field, the location or function (an
    /// entry's 32-hex id, fixed), else the property bag (the tag and its flag have their own rules
    /// and are not set here). Used where a bag of values (a
    /// symbol's defaults, an item's columns) becomes a device.
    ///
    /// # Errors
    /// As [`Device::set_text`].
    pub fn set_value(&mut self, key: &str, value: Option<&str>) -> std::result::Result<(), crate::naming::text::TextError> {
        // A location or function given as an entry's id (32 hex): set by hand, so manual.
        let id = || value.and_then(crate::naming::id::hex_id);
        match key {
            crate::document::book::LOCATION_KEY => {
                self.location = crate::document::book::Place::manual(id().map(crate::naming::id::LocationId));
                return Ok(());
            }
            crate::document::book::FUNCTION_KEY => {
                self.function = crate::document::book::Place::manual(id().map(crate::naming::id::FunctionId));
                return Ok(());
            }
            _ => {}
        }
        match crate::TextField::of_key(key) {
            Some(f) => self.set_text(f, value),
            None => {
                match value {
                    Some(v) => self.properties.insert(key.to_string(), v.to_string()),
                    None => self.properties.remove(key),
                };
                Ok(())
            }
        }
    }

    /// Its tag as text.
    pub fn tag(&self) -> Option<&str> {
        self.tag.tag()
    }

    /// This device with bare tag text `tag` in `state` (`None` clears it; text that is not a valid
    /// tag clears it too, which callers rule out with [`crate::naming::tag::Tag::new`] first).
    pub fn with_tag(&self, tag: Option<&str>, state: crate::naming::tag::TagState) -> Device {
        Device { tag: crate::naming::tag::TagSlot::of_text(tag, state), ..self.clone() }
    }
}

/// How many conductors a connection point accepts: 1 to
/// [`Accepts::MAX`]; two unless its symbol says more (PE bars, earth studs). The wire order chains
/// through a point that accepts two and fans out (a star) from one that accepts more; a point
/// with more wires than it accepts is a Checks error. Any other number is a read error.
///
/// # Example
/// ```
/// use ecad_model::Accepts;
/// assert_eq!(Accepts::default().get(), 2);
/// assert_eq!(Accepts::new(6).map(Accepts::get), Some(6));
/// assert!(Accepts::new(0).is_none() && Accepts::new(1000).is_none());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Accepts(u8);

impl Accepts {
    /// The most conductors a point may accept.
    pub const MAX: u8 = 64;
    /// The usual point: two conductors, one in and one out.
    pub const TWO: Accepts = Accepts(2);

    /// `n` conductors, when 1 to [`Accepts::MAX`].
    pub fn new(n: u64) -> Option<Accepts> {
        u8::try_from(n).ok().filter(|n| (1..=Self::MAX).contains(n)).map(Accepts)
    }

    /// How many.
    pub fn get(self) -> usize {
        usize::from(self.0)
    }

    /// Whether it is the usual two (left out of the file).
    pub fn is_two(&self) -> bool {
        *self == Self::TWO
    }
}

impl Default for Accepts {
    fn default() -> Self {
        Self::TWO
    }
}

impl<'de> Deserialize<'de> for Accepts {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let n = u64::deserialize(d)?;
        Accepts::new(n).ok_or_else(|| serde::de::Error::custom(format!("a connection point accepts 1 to {} conductors, not {n}", Accepts::MAX)))
    }
}

/// What a connection point belongs to: a device, or a drawing aid (a
/// continuation's point).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionPointOwner {
    Device(DeviceId),
    Aid(crate::naming::id::AidId),
}

impl From<DeviceId> for ConnectionPointOwner {
    fn from(d: DeviceId) -> Self {
        Self::Device(d)
    }
}

impl From<crate::naming::id::AidId> for ConnectionPointOwner {
    fn from(a: crate::naming::id::AidId) -> Self {
        Self::Aid(a)
    }
}

impl ConnectionPointOwner {
    /// The device it belongs to, if a device owns it.
    pub fn device(self) -> Option<DeviceId> {
        match self {
            Self::Device(d) => Some(d),
            Self::Aid(_) => None,
        }
    }

    /// The drawing aid it belongs to, if an aid owns it.
    pub fn aid(self) -> Option<crate::naming::id::AidId> {
        match self {
            Self::Aid(a) => Some(a),
            Self::Device(_) => None,
        }
    }

    /// Its id in hex, for texts that name it when nothing better does.
    pub fn subject_hex(self) -> String {
        match self {
            Self::Device(d) => d.hex(),
            Self::Aid(a) => a.hex(),
        }
    }

    /// The subject whose representations draw it.
    pub fn subject(self) -> Subject {
        match self {
            Self::Device(d) => Subject::Device(d),
            Self::Aid(a) => Subject::Aid(a),
        }
    }
}

/// A connection point of a device or a drawing aid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionPoint {
    /// What it belongs to.
    pub owner: ConnectionPointOwner,
    pub label: Option<String>,
    /// The definition terminal this one was created from.
    pub key: Option<PointKey>,
    /// How many conductors it accepts, copied from its definition terminal when it is made.
    #[serde(default, skip_serializing_if = "Accepts::is_two")]
    pub accepts: Accepts,
    /// Whether it is a protective earth (PE) point: set from its symbol when it is made (a PE
    /// terminal block, a definition terminal keyed `PE`), never re-read from the symbol.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub protective_earth: bool,
}

impl ConnectionPoint {
    /// The device it belongs to; `None` for a drawing aid's point.
    pub fn device(&self) -> Option<DeviceId> {
        self.owner.device()
    }
}

/// A terminal of a definition, in the symbol's local frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionPointDef {
    pub key: PointKey,
    pub label: Option<String>,
    pub at: Pt,
    /// How many conductors the point accepts; copied to each [`ConnectionPoint`] made
    /// from it.
    #[serde(default, skip_serializing_if = "Accepts::is_two")]
    pub accepts: Accepts,
}
