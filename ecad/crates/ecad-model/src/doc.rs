//! The document's objects and their relationships (proposal §3 and §4).
//!
//! Stored intent (components, terminals, conductors, cables, definitions) and drawing content
//! (pages, layers, items, representations) live here. Derived results do not (see [`crate::derive`]).

use std::collections::BTreeMap;

use crate::index::{Index, referrers};

use serde::{Deserialize, Serialize};

use crate::book::PageNo;
use crate::device::DeviceType;
use crate::error::{ModelError, Result};
use crate::geom::{Placement, Pt};
use crate::id::{CableId, DefinitionId, DeviceId, FolderId, ItemId, LayerId, LibraryId, PageId, RepresentationId, RouteId, TerminalId, WireId};
use crate::page::{PageKind, PageTitle};
use crate::point::PointKey;
use crate::role::{PartRole, Role};
use crate::sheet::SheetSpec;
use crate::style::StyleRef;
use crate::text::Text;
use crate::units::Radians;
pub use crate::wire::{End, Wire};

/// Optional, free-form properties. A missing key is `Unset` (principle P7); an empty string is a
/// confirmed empty value.
pub type Properties = BTreeMap<String, String>;

/// A page of the project (design 072 §5.1).
///
/// What the page is ([`PageKind`]), its place in the book (`number`, `folder`) and its look
/// (`page_type`, `sheet`, `title_block`) are fields; `display` holds only presentation choices and
/// the defaults registry's page-level values. Every optional field is left out of records while
/// unset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub title: PageTitle,
    /// The designer's page number (`100`, `105a`); `None` shows the book position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<PageNo>,
    /// The folder it is in; `None` at the top level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<FolderId>,
    /// What the page is; set when it is made.
    #[serde(default, skip_serializing_if = "PageKind::is_schematic")]
    pub kind: PageKind,
    /// Its page type (a style of catalogue `page`, brief 054); `None` takes the type its kind
    /// implies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_type: Option<StyleRef>,
    /// Its own sheet (brief 019); `None` takes its page type's, else the project's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<SheetSpec>,
    /// The frame style whose title block it borrows (brief 055); `None` takes its page type's,
    /// else its frame's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_block: Option<StyleRef>,
    /// Its location (an entry of the project's list); `None` takes the project's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<crate::id::LocationId>,
    /// Its function (an entry of the project's list); `None` takes the project's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<crate::id::FunctionId>,
    /// Presentation choices and registry values at page level.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub display: Properties,
}

impl Page {
    /// A schematic page titled `title`, with nothing else set.
    pub fn new(title: PageTitle) -> Self {
        Page {
            title,
            number: None,
            folder: None,
            kind: PageKind::Schematic,
            page_type: None,
            sheet: None,
            title_block: None,
            location: None,
            function: None,
            display: Properties::new(),
        }
    }

    /// What differs between `self` (before) and `after`: the fields one by one, and the display
    /// keys whose value differs. The update maps read a `ModifyPage` through it.
    pub fn change<'a>(&'a self, after: &'a Page) -> PageChange<'a> {
        let display = self
            .display
            .keys()
            .chain(after.display.keys().filter(|k| !self.display.contains_key(*k)))
            .filter(|k| self.display.get(*k) != after.display.get(*k))
            .map(String::as_str)
            .collect();
        PageChange {
            title: self.title != after.title,
            number: self.number != after.number,
            folder: self.folder != after.folder,
            page_type: self.page_type != after.page_type,
            sheet: self.sheet != after.sheet,
            title_block: self.title_block != after.title_block,
            location: self.location != after.location,
            function: self.function != after.function,
            display,
        }
    }

    /// A schematic page titled `title`, if it is a valid title.
    ///
    /// # Errors
    /// Why the title was refused.
    pub fn titled(title: impl AsRef<str>) -> std::result::Result<Self, crate::text::TextError> {
        PageTitle::new(title.as_ref()).map(Page::new)
    }
}

/// What a `ModifyPage` changed ([`Page::change`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageChange<'a> {
    pub title: bool,
    pub number: bool,
    pub folder: bool,
    pub page_type: bool,
    pub sheet: bool,
    pub title_block: bool,
    pub location: bool,
    pub function: bool,
    /// The display keys whose value changed.
    pub display: Vec<&'a str>,
}

/// A folder of pages (brief 019, decision FP3): a name and a place in the tree. Pages name their
/// folder in their `folder` field; sibling order is the relative order in
/// [`Document::folder_order`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    pub name: String,
    /// The folder it is in; `None` at the top level.
    pub parent: Option<FolderId>,
    /// The number its pages are renumbered from (brief 054, FP7; design 072 §5.2); `None` keeps
    /// the run of the folder above. Left out of records while unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<PageNo>,
}

impl Folder {
    /// A folder named `name` in `parent`, with no start number.
    pub fn new(name: impl Into<String>, parent: Option<FolderId>) -> Folder {
        Folder { name: name.into(), parent, start: None }
    }
}

/// A layer. References use [`LayerId`]; the name is a property (DOC-02).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
}

/// Colour of an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    ByLayer,
    ByBlock,
    /// AutoCAD colour index 1..=255.
    Index(u8),
    /// 0xRRGGBB.
    Rgb(u32),
}

/// How an item is drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub layer: LayerId,
    pub color: Color,
    /// Linetype name (`ByLayer`, `Continuous`, …).
    pub linetype: String,
    /// Lineweight code: -1 ByLayer, -2 ByBlock, -3 default, else hundredths of a millimetre.
    pub lineweight: i16,
}

/// A drawn primitive in the ECAD-owned schema (proposal R3). Angles are radians.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Primitive {
    Line {
        a: Pt,
        b: Pt,
    },
    /// Straight segments only.
    Polyline {
        vertices: Vec<Pt>,
        closed: bool,
    },
    Arc {
        center: Pt,
        radius: f64,
        start: f64,
        end: f64,
    },
    Circle {
        center: Pt,
        radius: f64,
    },
    /// Single-line text in the standard style. `at` is its alignment point: the left end of the
    /// baseline unless `halign`/`valign` say otherwise (brief 031, FP4; both are left out of the
    /// file at left/baseline, so older files read and save the same).
    Text {
        at: Pt,
        height: f64,
        rotation: Radians,
        value: String,
        #[serde(default, skip_serializing_if = "is_default")]
        halign: TextHAlign,
        #[serde(default, skip_serializing_if = "is_default")]
        valign: TextVAlign,
    },
    Point {
        at: Pt,
    },
}

impl Primitive {
    /// The end points of an open path (line or open polyline), as conductor routes use them.
    pub fn path_ends(&self) -> Option<(Pt, Pt)> {
        match self {
            Primitive::Line { a, b } => Some((*a, *b)),
            Primitive::Polyline { vertices, closed: false } => Some((*vertices.first()?, *vertices.last()?)),
            _ => None,
        }
    }

    /// The same path with one end moved (`End::A` = first vertex).
    pub fn with_path_end(&self, end: End, to: Pt) -> Option<Primitive> {
        match self {
            Primitive::Line { a, b } => Some(if end == End::A { Primitive::Line { a: to, b: *b } } else { Primitive::Line { a: *a, b: to } }),
            Primitive::Polyline { vertices, closed: false } => {
                let mut v = vertices.clone();
                let slot = if end == End::A { v.first_mut()? } else { v.last_mut()? };
                *slot = to;
                Some(Primitive::Polyline { vertices: v, closed: false })
            }
            _ => None,
        }
    }

    pub(crate) fn is_finite(&self) -> bool {
        match self {
            Primitive::Line { a, b } => a.is_finite() && b.is_finite(),
            Primitive::Polyline { vertices, .. } => vertices.iter().all(|p| p.is_finite()),
            Primitive::Arc { center, radius, start, end } => center.is_finite() && radius.is_finite() && start.is_finite() && end.is_finite(),
            Primitive::Circle { center, radius } => center.is_finite() && radius.is_finite(),
            Primitive::Text { at, height, rotation, .. } => at.is_finite() && height.is_finite() && rotation.is_finite(),
            Primitive::Point { at } => at.is_finite(),
        }
    }
}

/// A text's horizontal alignment (brief 031, FP4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextHAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// A text's vertical alignment (brief 031, FP4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextVAlign {
    #[default]
    Baseline,
    Bottom,
    Middle,
    Top,
}

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    *t == T::default()
}

/// A placed instance of an embedded definition (symbol form, proposal §4.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolInstance {
    pub definition: DefinitionId,
    pub placement: Placement,
}

/// What an item is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ItemKind {
    Primitive(Primitive),
    Symbol(SymbolInstance),
}

/// One drawn item on a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub page: PageId,
    pub style: Style,
    pub kind: ItemKind,
}

/// A device (design 072 §2.1): a real part with its symbols, its connection points and its type.
///
/// # Semantics
/// What the device is lives in [`Device::device_type`], copied from its symbol's
/// [`Definition::device_type`] when the symbol is placed as a new device; it is never worked out
/// from the symbol, the tag or a property. The fields not yet typed (design 072 lane B steps 2–5)
/// are still in [`Device::properties`].
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    /// Its tag (`-K01`), if it has one (design 026; the identity of the device).
    pub tag: Option<crate::tag::Tag>,
    /// Whether the tag was typed and is kept by renumbering (design 026 FP1).
    pub tag_fixed: bool,
    /// What it is for, in words (`setprop` key `description`).
    pub description: Option<crate::device::Description>,
    /// Its main part's number (`part_number`).
    pub part_number: Option<crate::device::PartText>,
    /// Its main part's maker (`maker`).
    pub manufacturer: Option<crate::device::PartText>,
    /// Its main part's article number (`article_number`).
    pub article_number: Option<crate::device::PartText>,
    /// Its type text (`type`): a terminal strip's type notation reads it.
    pub type_text: Option<crate::device::PartText>,
    /// Its location (an entry of the project's list, design 072 §6.3); `None` has none.
    pub location: Option<crate::id::LocationId>,
    /// Whether the location was set by hand (typed, or a box drawn around it) and is kept when a
    /// box around it changes; `false` when it came from its page or a box (design 072 §6.3).
    pub location_fixed: bool,
    /// Its function (an entry of the project's list); `None` has none.
    pub function: Option<crate::id::FunctionId>,
    /// Whether the function was set by hand, as [`Device::location_fixed`].
    pub function_fixed: bool,
    /// Its rating note: text that did not read as its group's typed rating (design 072 §2.2).
    pub rating_note: Option<crate::device::RatingNote>,
    pub properties: Properties,
    /// The embedded definition its symbol representations use, if any.
    pub definition: Option<DefinitionId>,
    /// What the device is.
    pub device_type: DeviceType,
    /// Its values of the project's user fields, by field (design 072 §2.8); left out while empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub user: BTreeMap<crate::id::UserFieldId, crate::user::UserValue>,
}

impl Device {
    /// A new device drawn by `definition` (`id`), with the type the symbol declares.
    pub fn of_definition(id: DefinitionId, definition: &Definition, properties: Properties) -> Device {
        Device { properties, definition: Some(id), device_type: definition.device_type.clone(), ..Device::default() }
    }

    /// Whether its tag is fixed (FP1): flagged, and not provisional (a provisional tag never
    /// counts as fixed, overseer ruling, tester 026 bug 2).
    pub fn is_fixed(&self) -> bool {
        self.tag_fixed && !self.tag().is_some_and(crate::tag::is_provisional)
    }

    /// Whether `other` differs in a typed field (anything but the property bag and the
    /// definition, which have ops of their own).
    pub fn fields_differ(&self, other: &Device) -> bool {
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

    /// Whether `other` differs from this device in its location and function (and their `fixed`
    /// flags) only: what a view that reads neither can skip.
    pub fn only_place_differs(&self, other: &Device) -> bool {
        let bare = |d: &Device| Device { location: None, location_fixed: false, function: None, function_fixed: false, ..d.clone() };
        bare(self) == bare(other)
    }

    /// The value `setprop`'s key `key` names on this device (design 072 §10: the key stays a
    /// string and routes to the field): a typed field as its stored text, else the property.
    pub fn value(&self, key: &str) -> Option<String> {
        match key {
            crate::tag::FIXED_KEY => self.tag_fixed.then(|| crate::words::flag_word(true).to_string()),
            crate::book::LOCATION_KEY => self.location.map(crate::id::LocationId::hex),
            crate::book::FUNCTION_KEY => self.function.map(crate::id::FunctionId::hex),
            crate::book::LOCATION_FIXED_KEY => self.location_fixed.then(|| crate::words::flag_word(true).to_string()),
            crate::book::FUNCTION_FIXED_KEY => self.function_fixed.then(|| crate::words::flag_word(true).to_string()),
            _ if crate::device::Connector::is_key(key) => match &self.device_type {
                DeviceType::Connector(x) => x.value(key),
                _ => None,
            },
            crate::device::RATING_KEY => self.rating_shown(),
            _ => match (&self.device_type, key.strip_prefix(crate::device::RATING_OF_PREFIX)) {
                (DeviceType::Protection(crate::device::Protection::FuseBank(b)), Some(n)) => {
                    let n = n.parse::<u8>().ok()?;
                    b.outputs.get(&n).map(ToString::to_string).filter(|t| !t.is_empty())
                }
                (DeviceType::TerminalStrip(s), Some(p)) => s.fuse_rating(p),
                _ => self.get(key).map(str::to_string),
            },
        }
    }

    /// Its rating as shown (design 072 §2.2): its typed rating fields when one is set, else its
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
        if key == crate::tag::TAG_KEY {
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
    /// The [`crate::text::TextError`] of a value that is too long or holds a hidden character.
    pub fn set_text(&mut self, f: crate::TextField, value: Option<&str>) -> std::result::Result<(), crate::text::TextError> {
        use crate::TextField as F;
        use crate::device::{Description, PartText};
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
    pub fn with_text(&self, f: crate::TextField, value: Option<&str>) -> std::result::Result<Device, crate::text::TextError> {
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
    pub fn set_value(&mut self, key: &str, value: Option<&str>) -> std::result::Result<(), crate::text::TextError> {
        // A location or function given as an entry's id (32 hex): set by hand, so fixed.
        let id = || value.and_then(|v| u128::from_str_radix(v.trim(), 16).ok().filter(|_| v.trim().len() == 32));
        match key {
            crate::book::LOCATION_KEY => {
                self.location = id().map(crate::id::LocationId);
                self.location_fixed = self.location.is_some();
                return Ok(());
            }
            crate::book::FUNCTION_KEY => {
                self.function = id().map(crate::id::FunctionId);
                self.function_fixed = self.function.is_some();
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
        self.tag.as_ref().map(|t| t.as_str())
    }

    /// This device with tag `tag` (`None` clears it; text that is not a valid tag clears it too,
    /// which callers rule out with [`crate::tag::Tag::new`] first).
    pub fn with_tag(&self, tag: Option<&str>) -> Device {
        Device { tag: tag.and_then(|t| crate::tag::Tag::new(t).ok()), ..self.clone() }
    }
}

/// How many conductors a connection point accepts (design 072 §2.7, §4.4, F38): 1 to
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

/// What a connection point belongs to (design 072 §3): a device, or a drawing aid (a
/// continuation's point, §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalOwner {
    Device(DeviceId),
    Aid(crate::id::AidId),
}

impl From<DeviceId> for TerminalOwner {
    fn from(d: DeviceId) -> Self {
        Self::Device(d)
    }
}

impl From<crate::id::AidId> for TerminalOwner {
    fn from(a: crate::id::AidId) -> Self {
        Self::Aid(a)
    }
}

impl TerminalOwner {
    /// The device it belongs to, if a device owns it.
    pub fn device(self) -> Option<DeviceId> {
        match self {
            Self::Device(d) => Some(d),
            Self::Aid(_) => None,
        }
    }

    /// The drawing aid it belongs to, if an aid owns it.
    pub fn aid(self) -> Option<crate::id::AidId> {
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
pub struct Terminal {
    /// What it belongs to (design 072 §3).
    pub owner: TerminalOwner,
    pub label: Option<String>,
    /// The definition terminal this one was created from (LIB-02; design 072 §2.7).
    pub key: Option<PointKey>,
    /// How many conductors it accepts, copied from its definition terminal when it is made.
    #[serde(default, skip_serializing_if = "Accepts::is_two")]
    pub accepts: Accepts,
}

impl Terminal {
    /// The device it belongs to; `None` for a drawing aid's point.
    pub fn device(&self) -> Option<DeviceId> {
        self.owner.device()
    }
}

/// What a representation depicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Subject {
    Device(DeviceId),
    Wire(WireId),
    Cable(CableId),
    /// A route's drawing (design 072 F31).
    Route(RouteId),
    /// A drawing aid's drawing (design 072 §6.2).
    Aid(crate::id::AidId),
}

/// The two representation forms (proposal §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Form {
    /// Page items in page coordinates; terminal positions stored as connection points.
    Drawn,
    /// One symbol-instance item; terminal positions derived from its placement.
    Symbol,
}

/// A depiction of a subject on one page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Representation {
    pub subject: Subject,
    pub page: PageId,
    pub form: Form,
    /// What it is for its subject (design 072 §4.4, F3′): a device symbol's part, or a fixed role.
    pub role: Role,
    pub items: Vec<ItemId>,
    /// Drawn form: where each terminal sits on the page. Symbol form (brief 018, decision on
    /// I14): empty for a symbol of the component's own definition, whose terminals are matched to
    /// the definition by key; otherwise where each terminal sits in the **definition's local
    /// frame**, so the terminals follow the instance's placement.
    pub connection_points: BTreeMap<TerminalId, Pt>,
}

/// Where a cable core is (owner ruling, 2026-11-18): stored, never worked out from the
/// connections on read.
///
/// # Semantics
/// - `Unplaced`: the core is on no line yet.
/// - `Route`: the core is on a drawn line that carries no wire yet (a loose line); `landed` is the
///   connection point of the one end that has landed, when exactly one has.
/// - `Wire`: the core is that wire. The command that lands a line's second end moves its cores
///   from the route to the wire it forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorePosition {
    Unplaced,
    Route { route: RouteId, landed: Option<TerminalId> },
    Wire(WireId),
}

impl CorePosition {
    /// The wire, when the core is on one.
    pub fn wire(self) -> Option<WireId> {
        match self {
            CorePosition::Wire(w) => Some(w),
            CorePosition::Unplaced | CorePosition::Route { .. } => None,
        }
    }

    /// The route, when the core is on a line with no wire yet.
    pub fn route(self) -> Option<RouteId> {
        match self {
            CorePosition::Route { route, .. } => Some(route),
            CorePosition::Unplaced | CorePosition::Wire(_) => None,
        }
    }
}

/// What a cable core is for (owner ruling, 2026-11-18): stored apart from where it is, never
/// worked out from its position.
///
/// # Semantics
/// - `InUse`: the core is meant to carry a conductor. One left [`CorePosition::Unplaced`] is an
///   open core (a check).
/// - `Spare`: the core is ticked **Not used**; it may stay unplaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    #[default]
    InUse,
    Spare,
}

/// The longest core label.
pub const MAX_CORE_LABEL: usize = 32;

/// A core's label: `1`, `BN`, `GNYE`.
pub type CoreLabel = crate::text::Text<MAX_CORE_LABEL>;

/// One core of a cable (design 072 §4.5): the cable's cores are its whole core set, so a core the
/// cable offers but no line carries yet is stored, [`CorePosition::Unplaced`].
///
/// # Semantics
/// `colours`, when set, are the core's own; `None` takes them from the cable type by core order.
/// `purpose` and `position` are independent: a spare core may sit on a line, an in-use core may be
/// unplaced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Core {
    pub label: Option<CoreLabel>,
    pub colours: Option<crate::wire::Colours>,
    pub purpose: Purpose,
    pub position: CorePosition,
    /// The core's named points on the cable's references (design 072 §4.4, F4): one connection
    /// point per reference end, positioned by that reference's representation. Every point listed
    /// here joins the others, so a line landing on one page's point and a line landing on the other
    /// page's are one net, and the wire runs from device point to device point across the pages.
    /// They are aids: never a wire's end.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub through: Vec<TerminalId>,
}

impl Core {
    /// An in-use core on wire `w`.
    pub fn on_wire(w: WireId, label: Option<CoreLabel>) -> Self {
        Core { label, colours: None, purpose: Purpose::InUse, position: CorePosition::Wire(w), through: Vec::new() }
    }

    /// An in-use core on no line yet.
    pub fn unplaced(label: Option<CoreLabel>) -> Self {
        Core { label, colours: None, purpose: Purpose::InUse, position: CorePosition::Unplaced, through: Vec::new() }
    }

    /// The label's text, when it has one.
    pub fn label_str(&self) -> Option<&str> {
        self.label.as_ref().map(CoreLabel::as_str)
    }

    /// Whether the core is ticked Not used.
    pub fn is_spare(&self) -> bool {
        self.purpose == Purpose::Spare
    }

    /// The core once its line is gone (a write rule, never a read): a labelled or spare core is one
    /// of the cable's core set and stays, unplaced; an unlabelled in-use core was only that line's
    /// membership and goes. `None` too when `others` already hold an unplaced core of that label.
    pub fn off_line(&self, others: &[Core]) -> Option<Core> {
        // A core with named points on the cable's references stays (F4): they are placed there.
        if self.label.is_none() && !self.is_spare() && self.through.is_empty() {
            return None;
        }
        let taken = self.label.is_some() && others.iter().any(|o| o.position == CorePosition::Unplaced && o.label == self.label);
        (!taken).then(|| Core { position: CorePosition::Unplaced, ..self.clone() })
    }

    /// The wire this core is, when it is one.
    pub fn wire(&self) -> Option<WireId> {
        self.position.wire()
    }

    /// The route this core is on, when it has no wire yet.
    pub fn route(&self) -> Option<RouteId> {
        self.position.route()
    }
}

/// The style-choice index key of a cable's cable type ([`Cable::cable_type`]); never a property.
pub const CABLE_TYPE_KEY: &str = "cable_type";

/// A cable groups existing conductors (DOC-04).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Cable {
    pub properties: Properties,
    /// The whole core set, in core order (design 072 §4.5).
    pub cores: Vec<Core>,
    /// Its cable type (a style of catalogue `cable`, design 072 §4.5): what its size, colours and
    /// shield come from when it has none of its own. A reference to no type is kept (a check row).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cable_type: Option<crate::style::StyleRef>,
    /// The cable's own shield; `None` takes its cable type's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shield: Option<crate::style::cable::Shield>,
    /// The cable's own rating inputs (design 057 §6.1, 072 §4.5); each overrides the project's.
    #[serde(default, skip_serializing_if = "crate::wire::CircuitInputs::is_empty")]
    pub rating: crate::wire::CircuitInputs,
    /// The design current typed for the cable (`load`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<crate::units::Amps>,
    /// The device named as its protection (`rating {protected_by}`), over the trace's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_by: Option<crate::wire::ProtectedBy>,
}

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
    /// above [`crate::tag::MAX_PADDING`] count as that maximum.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_padding: Option<u8>,
    /// The project's own tag prefix per symbol (`starter:coil → KA`, `project:valve-x → YV`),
    /// overriding a starter's built-in IEC 81346 default or a project symbol's own (design 072 F5).
    /// Empty by default.
    pub tag_prefixes: BTreeMap<crate::prefix::PrefixOf, crate::prefix::TagPrefix>,
    /// The prefix of new cable tags, when the project chose one (brief 026; design 072 F5: no
    /// longer a `tag_prefixes` entry). `None` follows the built-in `W`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cable_prefix: Option<crate::prefix::TagPrefix>,
    /// How wires are numbered and what each potential defaults to (brief 022, FP1). Left out of
    /// the file while it holds [`WiringSettings::default`].
    pub wiring: crate::wiring::WiringSettings,
    /// The project's sheet: size and grid of every page's frame unless a page sets its own
    /// (brief 019, decision FP2), when the project chose one (brief 066, F3: `None` follows the
    /// built-in, [`ProjectSettings::sheet`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet: Option<crate::sheet::SheetSpec>,
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
/// ([`crate::derive::place_text`]). Other styles (US) come later.
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
    pub fn starter_prefix(&self, word: &str) -> Option<&crate::prefix::TagPrefix> {
        self.tag_prefixes.get(&crate::prefix::PrefixOf::Starter(word.into()))
    }

    /// The project's own prefix for project symbol `key` (design 072 F5).
    pub fn project_prefix(&self, key: &str) -> Option<&crate::prefix::TagPrefix> {
        self.tag_prefixes.get(&crate::prefix::PrefixOf::Project(key.into()))
    }

    /// The tag padding: the project's choice, else the built-in ([`crate::builtins`]).
    pub fn tag_padding(&self) -> u8 {
        self.tag_padding.unwrap_or_else(|| crate::builtins::builtins().tag_padding)
    }

    /// The project sheet: the project's choice when it is within the limits, else the built-in.
    pub fn sheet(&self) -> crate::sheet::SheetSpec {
        self.sheet.filter(|s| s.check().is_ok()).unwrap_or(crate::builtins::builtins().sheet)
    }

    /// The page number step, at least 1: the project's choice, else the built-in.
    pub fn page_number_step(&self) -> u32 {
        self.page_number_step.unwrap_or_else(|| crate::builtins::builtins().page_number_step).max(1)
    }

    /// The reference style: the project's choice, else the built-in.
    pub fn reference_style(&self) -> ReferenceStyle {
        self.reference_style.unwrap_or_else(|| crate::builtins::builtins().reference_style)
    }
}

/// A terminal of a definition, in the symbol's local frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalDef {
    pub key: PointKey,
    pub label: Option<String>,
    pub at: Pt,
    /// How many conductors the point accepts (design 072 F38); copied to each [`Terminal`] made
    /// from it.
    #[serde(default, skip_serializing_if = "Accepts::is_two")]
    pub accepts: Accepts,
}

/// The device text a symbol slot places (design 048 §2.3, design 072 §7): one of the device's own
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
    pub const MAX_LABEL: usize = 64;

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

/// Where a device text of a placed copy goes, in the symbol's frame (brief 048, FP1; proposal
/// §4.2's text slots). An anchor only: size and style come from the text styles.
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

/// How a placed copy of a symbol joins a device (brief 048, FP2).
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
    Text { which: &'static str, error: crate::text::TextError },
}

/// One of a symbol's own default values (design 072 §7), by name.
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

/// A symbol's own default values (brief 048, FP2; design 072 §7): typed fields, each left out of
/// the file when not set. The starters have none.
///
/// # Semantics
/// - `key` names a project symbol; `prefix` is a new device's tag prefix; `description` and
///   `rating` are a new device's texts (the rating read against the definition's device type when
///   the symbol is saved); `join` says whether a placed copy is a device of its own or a part of a
///   named one; `aliases` are search words; `family` groups symbols in the library.
/// - Text values are kept trimmed and never empty: [`SymbolDefaults::set`] clears a value given as
///   blank. Lengths and shapes are the symbol command's rules (design 048 §3.5).
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

/// A project-local copy of a library definition (§4.2).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    pub origin: Option<Origin>,
    pub graphics: Vec<Primitive>,
    pub terminals: Vec<TerminalDef>,
    /// Groups of terminal keys joined inside the device (brief 022, FP5): every terminal of one
    /// group is on one net. Empty for almost every definition, and then left out of the file so
    /// existing content hashes do not change. A bridge may name a key the placement writes rather
    /// than one of the definition's own (brief 044, FP2: a connector half bridges the side keys
    /// `.s` and `.p`; [`crate::invariant::is_placement_key`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bridges: Vec<Vec<PointKey>>,
    /// Where the device texts of a placed copy go (brief 048, FP1). Empty for the starters, whose
    /// anchors are catalogue code; left out of the file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<TextSlot>,
    /// The symbol's own default values (brief 048, FP2; design 072 §7). Empty for the starters;
    /// left out of the file when empty.
    #[serde(default, skip_serializing_if = "SymbolDefaults::is_empty")]
    pub defaults: SymbolDefaults,
    /// The origin of the definition an edited copy came from (brief 048, FP4). Never followed by
    /// the reader, so a cycle cannot loop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<Origin>,
    /// What a device this symbol starts is (design 072 §2.3, F15): placing the symbol as a new
    /// device copies it. Left out of the file for [`DeviceType::General`].
    #[serde(default, skip_serializing_if = "DeviceType::is_general")]
    pub device_type: DeviceType,
    /// A placed copy's part in its device (design 072 §4.4, F3′): copied into the
    /// representation's [`Role::Part`] when the symbol is placed. Left out of the file for
    /// [`PartRole::Other`].
    #[serde(default, skip_serializing_if = "PartRole::is_other")]
    pub part_role: PartRole,
}

impl Definition {
    /// The keys bridged to `key` inside the device (not `key` itself), from [`Definition::bridges`].
    pub fn bridged_with<'a>(&'a self, key: &'a PointKey) -> impl Iterator<Item = &'a PointKey> + 'a {
        self.bridges.iter().filter(move |g| g.contains(key)).flat_map(|g| g.iter()).filter(move |k| *k != key)
    }
}

/// The authoritative document: everything stored, nothing derived.
///
/// # Semantics
/// Maps are ordered by ID so iteration, serialisation and comparison are deterministic.
/// [`Document::check`] verifies every invariant and [`Document::check_touched`] those a change
/// could have affected; [`crate::ledger::Ledger`] runs the scoped check after each transaction so
/// an invalid state is never committed (see [`crate::invariant`]).
///
/// The document also keeps reverse indexes (the private `index` module) for queries such as
/// [`Document::cables_of`]. They are derived: not serialised, not compared by `==`, and kept up
/// to date by [`crate::Op::apply`]. Code that edits the public maps directly must call
/// [`Document::rebuild_index`] before querying.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(from = "DocumentData")]
pub struct Document {
    pub pages: BTreeMap<PageId, Page>,
    pub page_order: Vec<PageId>,
    pub layers: BTreeMap<LayerId, Layer>,
    pub items: BTreeMap<ItemId, Item>,
    pub devices: BTreeMap<DeviceId, Device>,
    pub terminals: BTreeMap<TerminalId, Terminal>,
    pub representations: BTreeMap<RepresentationId, Representation>,
    pub wires: BTreeMap<WireId, Wire>,
    pub cables: BTreeMap<CableId, Cable>,
    /// The lines drawn on pages (design 072 F31).
    pub routes: BTreeMap<RouteId, crate::route::Route>,
    /// The drawing aids: continuations and location boxes (design 072 §6.2).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub aids: BTreeMap<crate::id::AidId, crate::aid::Aid>,
    /// The typed links between devices (design 072 §2.6).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub device_links: BTreeMap<crate::id::LinkId, crate::user::DeviceLink>,
    pub definitions: BTreeMap<DefinitionId, Definition>,
    pub settings: ProjectSettings,
    /// Named versions (bookmarks): name → the `seq` it names (§6.1). History, never pruned.
    pub versions: BTreeMap<String, u64>,
    /// The project's own properties (brief 019, decision FP1): title, drawing number, customer,
    /// status, default function and location, revisions (`rev.<n>.<field>`) and free ones.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: Properties,
    /// The project's own data: the location and function lists (design 072 §5.3).
    #[serde(skip_serializing_if = "crate::project::Project::is_empty")]
    pub project: crate::project::Project,
    /// Folders of pages (brief 019, decision FP3).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub folders: BTreeMap<FolderId, Folder>,
    /// Every folder once; siblings are ordered as they appear here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub folder_order: Vec<FolderId>,
    /// The project's standard (brief 050a, FP1); `None` is IEC (built in).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standard: Option<crate::style::Standard>,
    /// The project layer of style records (brief 050a, FP2).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub styles: crate::style::StyleCatalogues,
    /// Derived reverse indexes; never serialised.
    #[serde(skip)]
    pub(crate) index: Index,
    /// Derived nets (brief 022), kept by [`crate::Op::apply`]; never serialised.
    #[serde(skip)]
    pub(crate) conn: crate::connectivity::Connectivity,
}

/// The serialised form of [`Document`]: its stored fields, without the derived index.
#[derive(Deserialize)]
struct DocumentData {
    pages: BTreeMap<PageId, Page>,
    page_order: Vec<PageId>,
    layers: BTreeMap<LayerId, Layer>,
    items: BTreeMap<ItemId, Item>,
    devices: BTreeMap<DeviceId, Device>,
    terminals: BTreeMap<TerminalId, Terminal>,
    representations: BTreeMap<RepresentationId, Representation>,
    wires: BTreeMap<WireId, Wire>,
    cables: BTreeMap<CableId, Cable>,
    #[serde(default)]
    routes: BTreeMap<RouteId, crate::route::Route>,
    #[serde(default)]
    aids: BTreeMap<crate::id::AidId, crate::aid::Aid>,
    #[serde(default)]
    device_links: BTreeMap<crate::id::LinkId, crate::user::DeviceLink>,
    definitions: BTreeMap<DefinitionId, Definition>,
    settings: ProjectSettings,
    versions: BTreeMap<String, u64>,
    #[serde(default)]
    properties: Properties,
    #[serde(default)]
    project: crate::project::Project,
    #[serde(default)]
    folders: BTreeMap<FolderId, Folder>,
    #[serde(default)]
    folder_order: Vec<FolderId>,
    #[serde(default)]
    standard: Option<crate::style::Standard>,
    #[serde(default)]
    styles: crate::style::StyleCatalogues,
}

impl From<DocumentData> for Document {
    fn from(d: DocumentData) -> Self {
        let DocumentData {
            pages,
            page_order,
            layers,
            items,
            devices,
            terminals,
            representations,
            wires,
            cables,
            routes,
            aids,
            device_links,
            definitions,
            settings,
            versions,
            properties,
            project,
            folders,
            folder_order,
            standard,
            styles,
        } = d;
        let mut doc = Document {
            pages,
            page_order,
            layers,
            items,
            devices,
            terminals,
            representations,
            wires,
            cables,
            routes,
            aids,
            device_links,
            definitions,
            settings,
            versions,
            properties,
            project,
            folders,
            folder_order,
            standard,
            styles,
            index: Index::default(),
            conn: crate::connectivity::Connectivity::default(),
        };
        doc.rebuild_index();
        doc
    }
}

/// Documents are equal when their stored objects are equal; the derived index is not compared.
impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        let Document {
            pages,
            page_order,
            layers,
            items,
            devices,
            terminals,
            representations,
            wires,
            cables,
            routes,
            aids,
            device_links,
            definitions,
            settings,
            versions,
            properties,
            project,
            folders,
            folder_order,
            standard,
            styles,
            index: _,
            conn: _,
        } = self;
        *pages == other.pages
            && *page_order == other.page_order
            && *layers == other.layers
            && *items == other.items
            && *devices == other.devices
            && *terminals == other.terminals
            && *representations == other.representations
            && *wires == other.wires
            && *cables == other.cables
            && *routes == other.routes
            && *aids == other.aids
            && *device_links == other.device_links
            && *definitions == other.definitions
            && *settings == other.settings
            && *versions == other.versions
            && *properties == other.properties
            && *project == other.project
            && *folders == other.folders
            && *folder_order == other.folder_order
            && *standard == other.standard
            && *styles == other.styles
    }
}

impl Document {
    /// Recompute the reverse indexes from the stored objects.
    ///
    /// # Semantics
    /// Needed only after changing the public maps directly; [`crate::Op::apply`] keeps the index
    /// current on its own.
    pub fn rebuild_index(&mut self) {
        self.index = Index::build(self);
        self.conn = crate::connectivity::Connectivity::build(self);
    }

    /// Whether the index built on threads (as on open) equals one built on a single thread.
    /// For tests.
    #[doc(hidden)]
    pub fn index_build_is_deterministic(&self) -> bool {
        Index::build(self) == Index::build_serial(self)
    }

    /// Whether the kept index equals one rebuilt from the stored objects.
    ///
    /// # Errors
    /// [`ModelError::Invariant`] when the index is stale.
    pub fn check_index(&self) -> Result<()> {
        if self.index != Index::build(self) {
            return Err(ModelError::Invariant("the reverse indexes differ from the stored objects".into()));
        }
        // The kept nets (brief 022) are derived the same way: they must equal a fresh build.
        if let Some(diff) = self.conn.differs_from(&crate::connectivity::Connectivity::build(self)) {
            return Err(ModelError::Invariant(format!("the kept nets differ from a fresh build: {diff}")));
        }
        Ok(())
    }

    /// The nets, kept up to date by every op (brief 022).
    pub fn connectivity(&self) -> &crate::connectivity::Connectivity {
        &self.conn
    }

    /// Continuations (drawing aids) whose name has the key `key`
    /// ([`crate::connectivity::name_key`]), in ID order, through the name index.
    pub fn continuations_named(&self, key: &str) -> Vec<crate::id::AidId> {
        referrers(&self.index.continuations, &key.to_string()).collect()
    }

    /// Every continuation name key and its continuations, through the name index.
    pub fn continuation_names(&self) -> impl Iterator<Item = (&str, crate::id::AidId)> {
        self.index.continuations.iter().flat_map(|(k, ids)| ids.keys().map(move |c| (k.as_str(), *c)))
    }

    /// Components made from `definition`, in ID order.
    pub fn components_using(&self, definition: DefinitionId) -> Vec<DeviceId> {
        referrers(&self.index.definition_components, &definition).collect()
    }

    /// Symbol items placing `definition`, in ID order (brief 048: a definition's placed copies).
    pub fn items_using(&self, definition: DefinitionId) -> Vec<ItemId> {
        referrers(&self.index.definition_items, &definition).collect()
    }

    /// The representation an item belongs to, if any (DOC-05 allows at most one).
    pub fn representation_of_item(&self, item: ItemId) -> Option<(RepresentationId, &Representation)> {
        referrers(&self.index.item_reps, &item).find_map(|id| self.representations.get(&id).map(|r| (id, r)))
    }

    /// Representations of `subject`, in ID order.
    pub fn representations_of(&self, subject: Subject) -> impl Iterator<Item = (RepresentationId, &Representation)> {
        referrers(&self.index.subject_reps, &subject).filter_map(|id| self.representations.get(&id).map(|r| (id, r)))
    }

    /// Representations on `page`, in ID order (through the page index, without going through the
    /// whole document).
    pub fn representations_on(&self, page: PageId) -> impl Iterator<Item = (RepresentationId, &Representation)> {
        referrers(&self.index.page_reps, &page).filter_map(|id| self.representations.get(&id).map(|r| (id, r)))
    }

    /// Items on `page`, in ID order (through the page index, without going through the whole
    /// document).
    pub fn items_on(&self, page: PageId) -> impl Iterator<Item = (ItemId, &Item)> {
        referrers(&self.index.page_items, &page).filter_map(|id| self.items.get(&id).map(|i| (id, i)))
    }

    /// Representations that show `terminal` through an explicit connection point, in ID order
    /// (brief 036: a level terminal is always shown that way).
    pub fn representations_showing(&self, terminal: TerminalId) -> impl Iterator<Item = (RepresentationId, &Representation)> {
        referrers(&self.index.terminal_reps, &terminal).filter_map(|id| self.representations.get(&id).map(|r| (id, r)))
    }

    /// The definition a symbol representation places (its one symbol instance), if any.
    pub fn rep_definition(&self, r: &Representation) -> Option<&Definition> {
        r.items.iter().find_map(|i| match &self.items.get(i)?.kind {
            ItemKind::Symbol(s) => self.definitions.get(&s.definition),
            ItemKind::Primitive(_) => None,
        })
    }

    /// Connection points of `owner` (a device or a drawing aid), in ID order.
    pub fn terminals_of(&self, owner: impl Into<TerminalOwner>) -> impl Iterator<Item = (TerminalId, &Terminal)> {
        referrers(&self.index.component_terminals, &owner.into()).filter_map(|id| self.terminals.get(&id).map(|t| (id, t)))
    }

    /// Whether any conductor end is attached to `terminal`, through the index, without listing them.
    pub fn has_ends(&self, terminal: TerminalId) -> bool {
        self.index.terminal_ends.get(&terminal).is_some_and(|m| !m.is_empty())
    }

    /// Wire ends attached to `terminal`, in conductor ID order, end A before end B.
    pub fn ends_at(&self, terminal: TerminalId) -> Vec<(WireId, End)> {
        referrers(&self.index.terminal_ends, &terminal).collect()
    }

    /// The wires of net `net`, in ID order, through the index.
    pub fn wires_of_net(&self, net: crate::id::NetId) -> impl Iterator<Item = WireId> + '_ {
        referrers(&self.index.net_wires, &net)
    }

    /// The routes with an end on connection point `t`, in ID order, through the index.
    pub fn routes_at(&self, t: TerminalId) -> impl Iterator<Item = crate::id::RouteId> + '_ {
        referrers(&self.index.terminal_routes, &t)
    }

    /// The routes ending on the middle of route `r` (its T-joins), in ID order, through the index.
    pub fn tees_of(&self, r: crate::id::RouteId) -> impl Iterator<Item = crate::id::RouteId> + '_ {
        referrers(&self.index.route_tees, &r)
    }

    /// The routes drawn on page `page`, in ID order, through the index.
    pub fn routes_on(&self, page: PageId) -> impl Iterator<Item = crate::id::RouteId> + '_ {
        referrers(&self.index.page_routes, &page)
    }

    /// Components whose tag is the same tag as `tag` (equal [`crate::tag::key`]), in ID order,
    /// through the tag index: the cost does not grow with the project.
    pub fn components_tagged(&self, tag: &str) -> Vec<DeviceId> {
        referrers(&self.index.tags, &crate::tag::key(tag)).collect()
    }

    /// Every tagged component through the tag index (brief 018), by tag key ([`crate::tag::key`]),
    /// then ID: no scan of the components.
    pub fn tagged(&self) -> impl Iterator<Item = (&str, DeviceId)> {
        self.index.tags.iter().flat_map(|(k, ids)| ids.keys().map(move |c| (k.as_str(), *c)))
    }

    /// Cables whose tag is the same tag as `tag` (brief 026), in ID order, through the tag index.
    pub fn cables_tagged(&self, tag: &str) -> Vec<CableId> {
        referrers(&self.index.cable_tags, &crate::tag::key(tag)).collect()
    }

    /// Every tagged cable through the tag index (brief 026), by tag key, then ID.
    pub fn tagged_cables(&self) -> impl Iterator<Item = (&str, CableId)> {
        self.index.cable_tags.iter().flat_map(|(k, ids)| ids.keys().map(move |c| (k.as_str(), *c)))
    }

    /// Tag keys starting with `start` held by devices or cables (brief 026), through a range of
    /// the tag indexes: the cost follows the keys with that start, not the project.
    pub fn tag_keys_starting(&self, start: &str) -> impl Iterator<Item = &str> {
        let from = start.to_string();
        let a = self.index.tags.range(from.clone()..).map(|(k, _)| k.as_str());
        let b = self.index.cable_tags.range(from..).map(|(k, _)| k.as_str());
        let s1 = start.to_string();
        let s2 = start.to_string();
        a.take_while(move |k| k.starts_with(s1.as_str())).chain(b.take_while(move |k| k.starts_with(s2.as_str())))
    }

    /// Whether a device or a cable holds `tag` (brief 026: one namespace).
    pub fn tag_taken(&self, tag: &str) -> bool {
        let k = crate::tag::key(tag);
        self.index.tags.get(&k).is_some_and(|m| !m.is_empty()) || self.index.cable_tags.get(&k).is_some_and(|m| !m.is_empty())
    }

    /// Pages whose `folder` property names `folder`, in ID order (through the folder index).
    pub fn pages_in_folder(&self, folder: FolderId) -> Vec<PageId> {
        referrers(&self.index.folder_pages, &folder).collect()
    }

    /// Folders whose parent is `folder`, in ID order (through the folder index).
    pub fn subfolders(&self, folder: FolderId) -> Vec<FolderId> {
        referrers(&self.index.folder_children, &folder).collect()
    }

    /// Objects whose own style choice `key` names the stored style ID `value` (brief 050a, the
    /// style's explicit users), in order, through the index: no scan of the project.
    pub fn style_users(&self, key: &str, value: &str) -> impl Iterator<Item = crate::op::ObjectRef> + '_ {
        referrers(&self.index.style_users, &(value.trim().to_string(), key.to_string()))
    }

    /// Every style choice an object holds, through the index (brief 050a): (stored style ID,
    /// choice key, object), by ID then key.
    pub fn style_choices(&self) -> impl Iterator<Item = (&str, &str, crate::op::ObjectRef)> + '_ {
        self.index.style_users.iter().flat_map(|((v, k), objs)| objs.keys().map(move |o| (v.as_str(), k.as_str(), *o)))
    }

    /// Every (stored style ID, choice key) with at least one explicit user, and its users, from
    /// `value` on (brief 050a), through the index.
    pub fn style_users_of(&self, value: &str) -> impl Iterator<Item = (&str, crate::op::ObjectRef)> + '_ {
        let v = value.trim().to_string();
        self.index
            .style_users
            .range((v.clone(), String::new())..)
            .take_while(move |((sv, _), _)| *sv == v)
            .flat_map(|((_, k), objs)| objs.keys().map(move |o| (k.as_str(), *o)))
    }

    /// Cables `conductor` is a member of, in ID order.
    pub fn cables_of(&self, conductor: WireId) -> Vec<CableId> {
        referrers(&self.index.conductor_cables, &conductor).collect()
    }

    /// The cables with a core on route `r` (a line with no wire yet).
    pub fn cables_on_route(&self, r: RouteId) -> Vec<CableId> {
        referrers(&self.index.route_cables, &r).collect()
    }

    /// The cable and core index whose named point `t` is (a point of a cable reference, design 072
    /// §4.4, F4), through the index.
    pub fn cable_point(&self, t: TerminalId) -> Option<(CableId, usize)> {
        let cable = referrers(&self.index.cable_points, &t).next()?;
        let index = self.cables.get(&cable)?.cores.iter().position(|c| c.through.contains(&t))?;
        Some((cable, index))
    }

    /// Whether `t` is a connection point: a component's terminal or a cable reference's point.
    pub fn has_point(&self, t: TerminalId) -> bool {
        self.terminals.contains_key(&t) || self.cable_point(t).is_some()
    }

    /// The named points joined to `t` (`t` included), when `t` is one (design 072 §4.4, F4, F5): a
    /// cable reference's point joins the core's points on the cable's other references; a
    /// continuation's point joins the points of every continuation with the same name (an unnamed
    /// continuation's, its own). `None` for a device's own point. Named points are aids: a wire
    /// runs through them, never to them, so it runs device point to device point across pages.
    pub fn named_points(&self, t: TerminalId) -> Option<Vec<TerminalId>> {
        if let Some((cable, i)) = self.cable_point(t) {
            return self.cables.get(&cable)?.cores.get(i).map(|c| c.through.clone());
        }
        let aid = self.terminals.get(&t)?.owner.aid()?;
        Some(self.continuation_points(aid, self.aids.get(&aid)?))
    }

    /// The points of continuation `id` (as `aid`, before or after an edit) and of its partners by
    /// name, in ID order.
    pub fn continuation_points(&self, id: crate::id::AidId, aid: &crate::aid::Aid) -> Vec<TerminalId> {
        let aids = match crate::connectivity::continuation_key(aid) {
            Some(k) => self.continuations_named(&k),
            None => Vec::new(),
        };
        let aids = aids.into_iter().chain(std::iter::once(id));
        let mut out: Vec<TerminalId> = aids.into_iter().flat_map(|a| self.terminals_of(a).map(|(t, _)| t)).collect();
        out.sort();
        out.dedup();
        out
    }
}
