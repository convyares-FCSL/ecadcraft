//! The records at [`crate::record::SCHEMA_VERSION`]: the file format's own types.
//!
//! These mirror `ecad-model` deliberately: the model may change shape without changing the
//! transaction records, while these types change only with a schema version. (Snapshot chunks
//! store the model types themselves; the crate doc says how the schema version covers them.) The model types a record holds as they are (a device's
//! `DeviceType`, `TagSlot`, `Place`, `UserValue`) are the format too; the crate doc says why and
//! how the golden fixture guards them. Every struct refuses unknown fields.
//! IDs use the model's ID types, whose serialised form (32 hex digits) is itself part of this
//! schema and fixed by the golden-file test.

use std::collections::BTreeMap;

use ecad_model as m;
use ecad_model::{
    CableId, ConnectionPointId, DefinitionId, DeviceId, FolderId, ItemId, LayerId, LibraryId, PageId, RepresentationId, RouteId, TxId, WireId,
};
use serde::{Deserialize, Serialize};

/// A point `[x, y]`.
pub type Pt = [f64; 2];

fn pt(p: m::Pt) -> Pt {
    [p.x, p.y]
}
fn mpt(p: Pt) -> m::Pt {
    m::Pt::new(p[0], p[1])
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub title: m::PageTitle,
    /// Every optional field is left out while unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<m::PageNo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<FolderId>,
    #[serde(default, skip_serializing_if = "m::PageKind::is_schematic")]
    pub kind: m::PageKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_type: Option<m::StyleRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<SheetSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_block: Option<m::StyleRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<m::LocationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<m::FunctionId>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub display: BTreeMap<String, String>,
}

/// A folder of pages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    pub name: String,
    pub parent: Option<FolderId>,
    /// Left out while unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<m::PageNo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GridLabels {
    Letters,
    Numbers,
}

/// A sheet, in whole millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetSpec {
    pub width: u32,
    pub height: u32,
    pub columns: u32,
    pub rows: u32,
    pub column_labels: GridLabels,
    pub row_labels: GridLabels,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceStyle {
    PageColumnRow,
    PageColumn,
    /// Page, then zone: `2/B5`.
    PageZone,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Color {
    ByLayer,
    ByBlock,
    Index(u8),
    Rgb(u32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Style {
    pub layer: LayerId,
    pub color: Color,
    pub linetype: String,
    pub lineweight: i16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Primitive {
    Line {
        a: Pt,
        b: Pt,
    },
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
    Text {
        at: Pt,
        height: f64,
        rotation: m::geometry::units::Radians,
        value: String,
        #[serde(default, skip_serializing_if = "is_left")]
        halign: m::TextHAlign,
        #[serde(default, skip_serializing_if = "is_baseline")]
        valign: m::TextVAlign,
    },
    Point {
        at: Pt,
    },
}

fn is_left(h: &m::TextHAlign) -> bool {
    *h == m::TextHAlign::Left
}

fn is_baseline(v: &m::TextVAlign) -> bool {
    *v == m::TextVAlign::Baseline
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub origin: Pt,
    /// Counter-clockwise.
    pub rotation: m::geometry::units::Radians,
    pub mirror: bool,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolInstance {
    pub definition: DefinitionId,
    pub placement: Placement,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ItemKind {
    Primitive(Primitive),
    Symbol(SymbolInstance),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub page: PageId,
    pub style: Style,
    pub kind: ItemKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    /// Validated on read (1 to 40 characters, no hidden character).
    pub tag: m::naming::tag::TagSlot,
    /// The text fields, validated on read.
    pub description: Option<m::objects::devices::Description>,
    pub part_number: Option<m::objects::devices::PartText>,
    pub manufacturer: Option<m::objects::devices::PartText>,
    pub article_number: Option<m::objects::devices::PartText>,
    pub type_text: Option<m::objects::devices::PartText>,
    /// Ids of the project's lists, each with how it came to be.
    pub location: m::document::book::Place<m::LocationId>,
    pub function: m::document::book::Place<m::FunctionId>,
    /// Text that did not read as the group's typed rating.
    pub rating_note: Option<m::objects::devices::RatingNote>,
    pub properties: BTreeMap<String, String>,
    pub definition: Option<DefinitionId>,
    /// What the device is: the model's own type, whose words and fields are
    /// strict.
    pub device_type: m::DeviceType,
    /// Its values of the project's user fields; left out while empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub user: BTreeMap<m::UserFieldId, m::UserValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionPoint {
    pub owner: m::ConnectionPointOwner,
    pub label: Option<String>,
    pub key: Option<m::PointKey>,
    /// How many conductors it accepts, 1 to 64; left out at two.
    #[serde(default, skip_serializing_if = "m::Accepts::is_two")]
    pub accepts: m::Accepts,
    /// Whether it is a protective earth point; left out when not.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub protective_earth: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Subject {
    Device(DeviceId),
    Wire(WireId),
    Cable(CableId),
    Route(RouteId),
    Aid(m::AidId),
}

/// A route: a line drawn on a page and what its ends land on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub page: PageId,
    pub points: Vec<Pt>,
    pub ends: [RouteEnd; 2],
}

/// What one end of a route lands on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RouteEnd {
    Point(ConnectionPointId),
    On { route: RouteId, at: Pt },
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Form {
    Drawn,
    Symbol,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Representation {
    pub subject: Subject,
    pub page: PageId,
    pub form: Form,
    pub role: m::Role,
    pub items: Vec<ItemId>,
    pub connection_points: BTreeMap<ConnectionPointId, Pt>,
}

/// A wire: its net, its conductor id, its place in the net's order, its two
/// connection points, its typed values (the model's own strict types) and its display choices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wire {
    pub net: m::NetId,
    pub conductor: m::ConductorId,
    pub net_order: u16,
    pub ends: [ConnectionPointId; 2],
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub order_fixed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire_type: Option<m::StyleRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<m::objects::wire::WireSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colours: Option<m::objects::wire::Colours>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<m::objects::wire::WireNumber>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub potential: Option<m::naming::text::Text<{ m::objects::wire::MAX_VALUE_CHARS }>>,
    #[serde(default, skip_serializing_if = "m::objects::wire::CircuitInputs::is_empty")]
    pub rating: m::objects::wire::CircuitInputs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<m::geometry::units::Amps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_by: Option<m::objects::wire::ProtectedBy>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub display: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Core {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<m::CoreLabel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colours: Option<m::objects::wire::Colours>,
    pub purpose: Purpose,
    pub position: CorePosition,
    /// The core's named points on the cable's references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub through: Vec<m::ConnectionPointId>,
}

/// What a cable core is for, as written.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    InUse,
    Spare,
}

impl From<m::Purpose> for Purpose {
    fn from(p: m::Purpose) -> Self {
        match p {
            m::Purpose::InUse => Purpose::InUse,
            m::Purpose::Spare => Purpose::Spare,
        }
    }
}

impl From<Purpose> for m::Purpose {
    fn from(p: Purpose) -> Self {
        match p {
            Purpose::InUse => m::Purpose::InUse,
            Purpose::Spare => m::Purpose::Spare,
        }
    }
}

/// Where a cable core is: stored as written.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CorePosition {
    Unplaced,
    Route {
        route: RouteId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        landed: Option<ConnectionPointId>,
    },
    Wire(WireId),
}

impl From<m::CorePosition> for CorePosition {
    fn from(p: m::CorePosition) -> Self {
        match p {
            m::CorePosition::Unplaced => CorePosition::Unplaced,
            m::CorePosition::Route { route, landed } => CorePosition::Route { route, landed },
            m::CorePosition::Wire(w) => CorePosition::Wire(w),
        }
    }
}

impl From<CorePosition> for m::CorePosition {
    fn from(p: CorePosition) -> Self {
        match p {
            CorePosition::Unplaced => m::CorePosition::Unplaced,
            CorePosition::Route { route, landed } => m::CorePosition::Route { route, landed },
            CorePosition::Wire(w) => m::CorePosition::Wire(w),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cable {
    /// Validated on read; a cable's tag, as a device's.
    pub tag: m::naming::tag::TagSlot,
    pub description: Option<m::objects::devices::Description>,
    pub properties: BTreeMap<String, String>,
    pub cores: Vec<Core>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cable_type: Option<m::StyleRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shield: Option<m::objects::styles::cable::Shield>,
    #[serde(default, skip_serializing_if = "m::objects::wire::CircuitInputs::is_empty")]
    pub rating: m::objects::wire::CircuitInputs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<m::geometry::units::Amps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_by: Option<m::objects::wire::ProtectedBy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub library: LibraryId,
    pub definition: DefinitionId,
    pub library_seq: u64,
    pub content_hash: m::ContentHash,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionPointDef {
    pub key: m::PointKey,
    pub label: Option<String>,
    pub at: Pt,
    /// How many conductors the point accepts, 1 to 64; left out at two, so a
    /// definition without it is written, and hashed, exactly as before.
    #[serde(default, skip_serializing_if = "m::Accepts::is_two")]
    pub accepts: m::Accepts,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub name: String,
    pub origin: Option<Origin>,
    pub graphics: Vec<Primitive>,
    pub connection_points: Vec<ConnectionPointDef>,
    /// Groups of terminal keys joined inside the device. Left out when empty, so
    /// a definition without bridges is written, and hashed, exactly as before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bridges: Vec<Vec<m::PointKey>>,
    /// Text slots. Left out when empty, so existing hashes hold.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<TextSlot>,
    /// The symbol's own defaults. Left out when empty.
    #[serde(default, skip_serializing_if = "m::SymbolDefaults::is_empty")]
    pub defaults: m::SymbolDefaults,
    /// The origin an edited copy came from. Left out when none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<Origin>,
    /// What a device this symbol starts is. Left out for `general`.
    #[serde(default, skip_serializing_if = "m::DeviceType::is_general")]
    pub device_type: m::DeviceType,
    /// A placed copy's part in its device. Left out for `other`.
    #[serde(default, skip_serializing_if = "m::PartRole::is_other")]
    pub part_role: m::PartRole,
}

/// A text slot of a definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextSlot {
    pub text: m::SlotText,
    pub at: Pt,
    #[serde(default, skip_serializing_if = "is_left")]
    pub h: m::TextHAlign,
    #[serde(default, skip_serializing_if = "is_baseline")]
    pub v: m::TextVAlign,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: m::geometry::units::Radians,
}

fn is_zero(x: &m::geometry::units::Radians) -> bool {
    // -0.0 too: the canonical form does not keep its sign, so writing it would give a content hash
    // that the read-back value (0.0, left out) does not match.
    x.0 == 0.0
}

fn origin(o: &m::Origin) -> Origin {
    Origin { library: o.library, definition: o.definition, library_seq: o.library_seq, content_hash: o.content_hash }
}

fn morigin(o: Origin) -> m::Origin {
    m::Origin { library: o.library, definition: o.definition, library_seq: o.library_seq, content_hash: o.content_hash }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LengthUnit {
    Millimetre,
    Inch,
}

/// Project settings. `tag_padding` and `tag_prefixes` are
/// left out while they hold their defaults, so a project that never changes them is written
/// exactly as before; a file that sets them is refused by older builds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSettings {
    pub length_unit: LengthUnit,
    /// Written only when the project chose a padding, even one equal to the
    /// built-in; absent follows the built-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_padding: Option<u8>,
    /// Keyed by the symbol, `starter:<word>` or `project:<key>`; read strictly.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tag_prefixes: BTreeMap<m::naming::prefix::PrefixOf, m::naming::prefix::TagPrefix>,
    /// The cable tag prefix, written only when the project chose one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cable_prefix: Option<m::naming::prefix::TagPrefix>,
    /// Wire numbering and potential defaults: left out at its default, so a
    /// project that never changes them is written exactly as before.
    #[serde(default, skip_serializing_if = "is_default_wiring")]
    pub wiring: WiringSettings,
    /// Written only when the project chose a sheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<SheetSpec>,
    /// Written only when the project chose a step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_number_step: Option<u32>,
    /// Written only when the project chose a style,
    /// `page_zone` included; absent follows the built-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_style: Option<ReferenceStyle>,
    /// Left out at the default `auto`.
    #[serde(default, skip_serializing_if = "ContactMirror::is_auto")]
    pub contact_mirror: ContactMirror,
}

/// Where contact mirror tables go by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactMirror {
    #[default]
    Auto,
    Below,
    Right,
    Off,
}

impl ContactMirror {
    fn is_auto(&self) -> bool {
        *self == ContactMirror::Auto
    }
}

fn is_default_wiring(w: &WiringSettings) -> bool {
    *w == WiringSettings::from(&m::connectivity::wiring::WiringSettings::default())
}

/// How automatic wire numbers are made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scheme {
    #[serde(rename = "potential")]
    Potential,
    #[serde(rename = "page")]
    Page,
    #[serde(rename = "page-column")]
    PageColumn,
}

/// One row of the potentials table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PotentialDef {
    pub name: String,
    pub prefix: String,
    pub numbered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supply: Option<m::connectivity::supply::Supply>,
}

/// Wiring settings. `colour_style` is left out at its default; the label height, rule
/// and shortest labelled wire are written only when the project chose them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WiringSettings {
    pub scheme: Scheme,
    /// Absent follows the built-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digits: Option<u8>,
    pub unclassified_prefix: String,
    pub potentials: Vec<PotentialDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour_style: Option<m::geometry::colour::ColourWords>,
    /// Written only when the project chose a height, even one equal to the
    /// built-in; absent follows the built-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_height: Option<f64>,
    /// Left out at its default, like its neighbours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_rule: Option<String>,
    /// Left out at its default, like its neighbours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_min_length: Option<f64>,
    /// What one wire number counts, `net` or `conductor`; left out at `net`.
    #[serde(default, skip_serializing_if = "m::connectivity::wiring::NumberBy::is_net")]
    pub number_by: m::connectivity::wiring::NumberBy,
}

impl Default for WiringSettings {
    fn default() -> Self {
        WiringSettings::from(&m::connectivity::wiring::WiringSettings::default())
    }
}

impl From<&m::connectivity::wiring::WiringSettings> for WiringSettings {
    fn from(w: &m::connectivity::wiring::WiringSettings) -> Self {
        WiringSettings {
            scheme: match w.scheme {
                m::connectivity::wiring::Scheme::Potential => Scheme::Potential,
                m::connectivity::wiring::Scheme::Page => Scheme::Page,
                m::connectivity::wiring::Scheme::PageColumn => Scheme::PageColumn,
            },
            digits: w.digits,
            unclassified_prefix: w.unclassified_prefix.clone(),
            potentials: w
                .potentials
                .iter()
                .map(|p| PotentialDef { name: p.name.clone(), prefix: p.prefix.clone(), numbered: p.numbered, supply: p.supply })
                .collect(),
            colour_style: w.colour_style,
            label_height: w.label_height,
            label_rule: w.label_rule.clone(),
            label_min_length: w.label_min_length,
            number_by: w.number_by,
        }
    }
}

fn labels(l: m::GridLabels) -> GridLabels {
    match l {
        m::GridLabels::Letters => GridLabels::Letters,
        m::GridLabels::Numbers => GridLabels::Numbers,
    }
}

fn mlabels(l: GridLabels) -> m::GridLabels {
    match l {
        GridLabels::Letters => m::GridLabels::Letters,
        GridLabels::Numbers => m::GridLabels::Numbers,
    }
}

impl From<&m::SheetSpec> for SheetSpec {
    fn from(s: &m::SheetSpec) -> Self {
        SheetSpec {
            width: s.width,
            height: s.height,
            columns: s.columns,
            rows: s.rows,
            column_labels: labels(s.column_labels),
            row_labels: labels(s.row_labels),
        }
    }
}

impl From<WiringSettings> for m::connectivity::wiring::WiringSettings {
    fn from(w: WiringSettings) -> Self {
        m::connectivity::wiring::WiringSettings {
            scheme: match w.scheme {
                Scheme::Potential => m::connectivity::wiring::Scheme::Potential,
                Scheme::Page => m::connectivity::wiring::Scheme::Page,
                Scheme::PageColumn => m::connectivity::wiring::Scheme::PageColumn,
            },
            digits: w.digits,
            unclassified_prefix: w.unclassified_prefix,
            potentials: w
                .potentials
                .into_iter()
                .map(|p| m::connectivity::wiring::PotentialDef { name: p.name, prefix: p.prefix, numbered: p.numbered, supply: p.supply })
                .collect(),
            colour_style: w.colour_style,
            label_height: w.label_height,
            label_rule: w.label_rule,
            label_min_length: w.label_min_length,
            number_by: w.number_by,
        }
    }
}

impl From<SheetSpec> for m::SheetSpec {
    fn from(s: SheetSpec) -> Self {
        m::SheetSpec {
            width: s.width,
            height: s.height,
            columns: s.columns,
            rows: s.rows,
            column_labels: mlabels(s.column_labels),
            row_labels: mlabels(s.row_labels),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ObjectRef {
    Device(DeviceId),
    Wire(WireId),
    Cable(CableId),
    Page(PageId),
    Project,
    /// The standard's registry values.
    Standard,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Author {
    User,
    Agent(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TxKind {
    Edit,
    Undo(TxId),
    Redo(TxId),
    /// A merge: `{"merge": {"source": hex, "source_head": {seq, chain},
    /// "ancestor": {branch, seq, chain}}}`. An older reader refuses the unknown kind.
    Merge(MergeKind),
}

/// Where a merge came from, as recorded (hex IDs and chain values).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeKind {
    pub source: Hex<16>,
    pub source_head: MergeStep,
    pub ancestor: MergeAncestor,
}

/// A step of the source: its `seq` and chain value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeStep {
    pub seq: u64,
    pub chain: Hex<32>,
}

/// The common ancestor: a branch, its step and chain value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeAncestor {
    pub branch: Hex<16>,
    pub seq: u64,
    pub chain: Hex<32>,
}

/// Bytes written as lower-case hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hex<const N: usize>(pub [u8; N]);

impl<const N: usize> Serialize for Hex<N> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.iter().map(|b| format!("{b:02x}")).collect::<String>())
    }
}

impl<'de, const N: usize> Deserialize<'de> for Hex<N> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let t = String::deserialize(d)?;
        let bad = || serde::de::Error::custom(format!("{t:?} is not {N} bytes of lower-case hex"));
        if t.len() != N * 2 || !t.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(bad());
        }
        let mut out = [0u8; N];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = t.get(i * 2..i * 2 + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).ok_or_else(bad)?;
        }
        Ok(Hex(out))
    }
}

/// An op, at [`OP_VERSION`]. In the file each op is written flat:
/// `{"op": "<Name>", "v": 1, …fields…}` (see [`crate::record`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Op {
    AddPage {
        id: PageId,
        page: Page,
        index: u64,
    },
    RemovePage {
        id: PageId,
        page: Page,
        index: u64,
    },
    ModifyPage {
        id: PageId,
        before: Page,
        after: Page,
    },
    AddLayer {
        id: LayerId,
        layer: Layer,
    },
    RemoveLayer {
        id: LayerId,
        layer: Layer,
    },
    AddItem {
        id: ItemId,
        item: Item,
    },
    ModifyItem {
        id: ItemId,
        before: Item,
        after: Item,
    },
    RemoveItem {
        id: ItemId,
        item: Item,
    },
    CreateDevice {
        id: DeviceId,
        device: Device,
    },
    DeleteDevice {
        id: DeviceId,
        device: Device,
    },
    ModifyDevice {
        id: DeviceId,
        before: Device,
        after: Device,
    },
    SetDeviceDefinition {
        id: DeviceId,
        before: Option<DefinitionId>,
        after: Option<DefinitionId>,
    },
    AddConnectionPoint {
        id: ConnectionPointId,
        connection_point: ConnectionPoint,
    },
    RemoveConnectionPoint {
        id: ConnectionPointId,
        connection_point: ConnectionPoint,
    },
    LinkRepresentation {
        id: RepresentationId,
        rep: Representation,
    },
    ModifyRepresentation {
        id: RepresentationId,
        before: Representation,
        after: Representation,
    },
    UnlinkRepresentation {
        id: RepresentationId,
        rep: Representation,
    },
    CreateWire {
        id: WireId,
        wire: Wire,
    },
    DeleteWire {
        id: WireId,
        wire: Wire,
    },
    ModifyWire {
        id: WireId,
        before: Wire,
        after: Wire,
    },
    AddRoute {
        id: RouteId,
        route: Route,
    },
    RemoveRoute {
        id: RouteId,
        route: Route,
    },
    ModifyRoute {
        id: RouteId,
        before: Route,
        after: Route,
    },
    CreateCable {
        id: CableId,
        cable: Cable,
    },
    DeleteCable {
        id: CableId,
        cable: Cable,
    },
    ModifyCable {
        id: CableId,
        before: Cable,
        after: Cable,
    },
    SetProperty {
        object: ObjectRef,
        key: String,
        before: Option<String>,
        after: Option<String>,
    },
    EmbedDefinition {
        id: DefinitionId,
        definition: Definition,
    },
    RemoveDefinition {
        id: DefinitionId,
        definition: Definition,
    },
    SetProjectSettings {
        before: ProjectSettings,
        after: ProjectSettings,
    },
    NameVersion {
        name: m::PointName,
        at: u64,
        /// A version's description; left out of the record when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    UnnameVersion {
        name: m::PointName,
        at: u64,
        /// A version's description; left out of the record when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    AddFolder {
        id: FolderId,
        folder: Folder,
        index: u64,
    },
    RemoveFolder {
        id: FolderId,
        folder: Folder,
        index: u64,
    },
    ModifyFolder {
        id: FolderId,
        before: Folder,
        after: Folder,
    },
    /// One whole style record; `null` is no record.
    SetStyle {
        layer: StyleLayer,
        catalogue: m::StyleCatalogue,
        id: m::StyleRef,
        before: Option<StyleRecord>,
        after: Option<StyleRecord>,
    },
    /// The standard's header; `null` is no standard (IEC built in).
    SetStandard {
        before: Option<StandardHeader>,
        after: Option<StandardHeader>,
    },
    /// The project's own data, the model's strict record.
    ModifyProject {
        before: m::Project,
        after: m::Project,
    },
    /// A link between two devices made or removed, the model's strict record.
    AddDeviceLink {
        id: m::LinkId,
        link: m::DeviceLink,
    },
    RemoveDeviceLink {
        id: m::LinkId,
        link: m::DeviceLink,
    },
    /// A drawing aid made, removed or changed, the model's strict record.
    CreateAid {
        id: m::AidId,
        aid: m::Aid,
    },
    DeleteAid {
        id: m::AidId,
        aid: m::Aid,
    },
    ModifyAid {
        id: m::AidId,
        before: m::Aid,
        after: m::Aid,
    },
    /// A loose part made, removed or changed, the model's strict record.
    CreateLoosePart {
        id: m::LoosePartId,
        part: m::objects::loose_part::LoosePart,
    },
    DeleteLoosePart {
        id: m::LoosePartId,
        part: m::objects::loose_part::LoosePart,
    },
    ModifyLoosePart {
        id: m::LoosePartId,
        before: m::objects::loose_part::LoosePart,
        after: m::objects::loose_part::LoosePart,
    },
}

/// Where a style record lives.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StyleLayer {
    Standard,
    Project,
}

/// A style record: the model's strict record, its typed body as is.
pub type StyleRecord = m::StyleRecord;

/// A standard's header: the model's strict record.
pub type StandardHeader = m::StandardHeader;

fn layer(l: m::StyleLayer) -> StyleLayer {
    match l {
        m::StyleLayer::Standard => StyleLayer::Standard,
        m::StyleLayer::Project => StyleLayer::Project,
    }
}

fn mlayer(l: StyleLayer) -> m::StyleLayer {
    match l {
        StyleLayer::Standard => m::StyleLayer::Standard,
        StyleLayer::Project => m::StyleLayer::Project,
    }
}

/// The version every op is written in; an op of any other version is refused.
pub const OP_VERSION: u64 = 1;

// ---------------------------------------------------------------------------------------------
// model → record

impl From<&m::Color> for Color {
    fn from(c: &m::Color) -> Self {
        match *c {
            m::Color::ByLayer => Color::ByLayer,
            m::Color::ByBlock => Color::ByBlock,
            m::Color::Index(i) => Color::Index(i),
            m::Color::Rgb(v) => Color::Rgb(v),
        }
    }
}

impl From<&m::Style> for Style {
    fn from(s: &m::Style) -> Self {
        Style { layer: s.layer, color: (&s.color).into(), linetype: s.linetype.clone(), lineweight: s.lineweight }
    }
}

impl From<&m::Primitive> for Primitive {
    fn from(p: &m::Primitive) -> Self {
        match p {
            m::Primitive::Line { a, b } => Primitive::Line { a: pt(*a), b: pt(*b) },
            m::Primitive::Polyline { vertices, closed } => {
                Primitive::Polyline { vertices: vertices.iter().copied().map(pt).collect(), closed: *closed }
            }
            m::Primitive::Arc { center, radius, start, end } => Primitive::Arc { center: pt(*center), radius: *radius, start: *start, end: *end },
            m::Primitive::Circle { center, radius } => Primitive::Circle { center: pt(*center), radius: *radius },
            m::Primitive::Text { at, height, rotation, value, halign, valign } => {
                Primitive::Text { at: pt(*at), height: *height, rotation: *rotation, value: value.clone(), halign: *halign, valign: *valign }
            }
            m::Primitive::Point { at } => Primitive::Point { at: pt(*at) },
        }
    }
}

impl From<&m::Item> for Item {
    fn from(i: &m::Item) -> Self {
        let kind = match &i.kind {
            m::ItemKind::Primitive(p) => ItemKind::Primitive(p.into()),
            m::ItemKind::Symbol(s) => ItemKind::Symbol(SymbolInstance {
                definition: s.definition,
                placement: Placement {
                    origin: pt(s.placement.origin),
                    rotation: s.placement.rotation,
                    mirror: s.placement.mirror,
                    scale: s.placement.scale,
                },
            }),
        };
        Item { page: i.page, style: (&i.style).into(), kind }
    }
}

impl From<&m::Device> for Device {
    fn from(c: &m::Device) -> Self {
        Device {
            tag: c.tag.clone(),
            description: c.description.clone(),
            part_number: c.part_number.clone(),
            manufacturer: c.manufacturer.clone(),
            article_number: c.article_number.clone(),
            type_text: c.type_text.clone(),
            location: c.location,
            function: c.function,
            rating_note: c.rating_note.clone(),
            properties: c.properties.clone(),
            definition: c.definition,
            device_type: c.device_type.clone(),
            user: c.user.clone(),
        }
    }
}

impl From<&m::ConnectionPoint> for ConnectionPoint {
    fn from(t: &m::ConnectionPoint) -> Self {
        ConnectionPoint { owner: t.owner, label: t.label.clone(), key: t.key.clone(), accepts: t.accepts, protective_earth: t.protective_earth }
    }
}

impl From<&m::Representation> for Representation {
    fn from(r: &m::Representation) -> Self {
        Representation {
            subject: match r.subject {
                m::Subject::Device(c) => Subject::Device(c),
                m::Subject::Wire(c) => Subject::Wire(c),
                m::Subject::Cable(c) => Subject::Cable(c),
                m::Subject::Route(c) => Subject::Route(c),
                m::Subject::Aid(c) => Subject::Aid(c),
            },
            page: r.page,
            form: match r.form {
                m::Form::Drawn => Form::Drawn,
                m::Form::Symbol => Form::Symbol,
            },
            role: r.role,
            items: r.items.clone(),
            connection_points: r.connection_points.iter().map(|(k, v)| (*k, pt(*v))).collect(),
        }
    }
}

impl From<&m::Wire> for Wire {
    fn from(c: &m::Wire) -> Self {
        Wire {
            net: c.net,
            conductor: c.conductor,
            net_order: c.net_order,
            ends: c.ends,
            order_fixed: c.order_fixed,
            wire_type: c.wire_type.clone(),
            size: c.size,
            colours: c.colours.clone(),
            number: c.number.clone(),
            potential: c.potential.clone(),
            rating: c.rating,
            load: c.load,
            protected_by: c.protected_by,
            display: c.display.clone(),
        }
    }
}

impl From<&m::Route> for Route {
    fn from(r: &m::Route) -> Self {
        let end = |e: m::RouteEnd| match e {
            m::RouteEnd::Point(t) => RouteEnd::Point(t),
            m::RouteEnd::On { route, at } => RouteEnd::On { route, at: pt(at) },
            m::RouteEnd::Free => RouteEnd::Free,
        };
        Route { page: r.page, points: r.points.iter().map(|p| pt(*p)).collect(), ends: r.ends.map(end) }
    }
}

impl From<Route> for m::Route {
    fn from(r: Route) -> Self {
        let end = |e: RouteEnd| match e {
            RouteEnd::Point(t) => m::RouteEnd::Point(t),
            RouteEnd::On { route, at } => m::RouteEnd::On { route, at: mpt(at) },
            RouteEnd::Free => m::RouteEnd::Free,
        };
        m::Route { page: r.page, points: r.points.into_iter().map(mpt).collect(), ends: r.ends.map(end) }
    }
}

impl From<&m::Core> for Core {
    fn from(c: &m::Core) -> Self {
        Core {
            label: c.label.clone(),
            colours: c.colours.clone(),
            purpose: c.purpose.into(),
            position: c.position.into(),
            through: c.through.clone(),
        }
    }
}

impl From<&m::Cable> for Cable {
    fn from(c: &m::Cable) -> Self {
        Cable {
            tag: c.tag.clone(),
            description: c.description.clone(),
            properties: c.properties.clone(),
            cores: c.cores.iter().map(Into::into).collect(),
            cable_type: c.cable_type.clone(),
            shield: c.shield,
            rating: c.rating,
            load: c.load,
            protected_by: c.protected_by,
        }
    }
}

impl From<&m::Definition> for Definition {
    fn from(d: &m::Definition) -> Self {
        Definition {
            name: d.name.clone(),
            origin: d.origin.as_ref().map(origin),
            graphics: d.graphics.iter().map(Into::into).collect(),
            connection_points: d
                .connection_points
                .iter()
                .map(|t| ConnectionPointDef { key: t.key.clone(), label: t.label.clone(), at: pt(t.at), accepts: t.accepts })
                .collect(),
            bridges: d.bridges.clone(),
            texts: d.texts.iter().map(|t| TextSlot { text: t.text.clone(), at: pt(t.at), h: t.h, v: t.v, rotation: t.rotation }).collect(),
            defaults: d.defaults.clone(),
            derived_from: d.derived_from.as_ref().map(origin),
            device_type: d.device_type.clone(),
            part_role: d.part_role,
        }
    }
}

impl From<&m::Page> for Page {
    fn from(p: &m::Page) -> Self {
        Page {
            title: p.title.clone(),
            number: p.number.clone(),
            folder: p.folder,
            kind: p.kind,
            page_type: p.page_type.clone(),
            sheet: p.sheet.as_ref().map(SheetSpec::from),
            title_block: p.title_block.clone(),
            location: p.location,
            function: p.function,
            display: p.display.clone(),
        }
    }
}

impl From<Page> for m::Page {
    fn from(p: Page) -> Self {
        m::Page {
            title: p.title,
            number: p.number,
            folder: p.folder,
            kind: p.kind,
            page_type: p.page_type,
            sheet: p.sheet.map(m::SheetSpec::from),
            title_block: p.title_block,
            location: p.location,
            function: p.function,
            display: p.display,
        }
    }
}

impl From<&m::Folder> for Folder {
    fn from(f: &m::Folder) -> Self {
        Folder { name: f.name.clone(), parent: f.parent, start: f.start.clone() }
    }
}

impl From<Folder> for m::Folder {
    fn from(f: Folder) -> Self {
        m::Folder { name: f.name, parent: f.parent, start: f.start }
    }
}

impl From<&m::ProjectSettings> for ProjectSettings {
    fn from(s: &m::ProjectSettings) -> Self {
        ProjectSettings {
            length_unit: match s.length_unit {
                m::LengthUnit::Millimetre => LengthUnit::Millimetre,
                m::LengthUnit::Inch => LengthUnit::Inch,
            },
            tag_padding: s.tag_padding,
            tag_prefixes: s.tag_prefixes.clone(),
            cable_prefix: s.cable_prefix.clone(),
            wiring: WiringSettings::from(&s.wiring),
            sheet: s.sheet.as_ref().map(SheetSpec::from),
            page_number_step: s.page_number_step,
            reference_style: s.reference_style.map(|r| match r {
                m::ReferenceStyle::PageColumnRow => ReferenceStyle::PageColumnRow,
                m::ReferenceStyle::PageColumn => ReferenceStyle::PageColumn,
                m::ReferenceStyle::PageZone => ReferenceStyle::PageZone,
            }),
            contact_mirror: match s.contact_mirror {
                m::ContactMirror::Auto => ContactMirror::Auto,
                m::ContactMirror::Below => ContactMirror::Below,
                m::ContactMirror::Right => ContactMirror::Right,
                m::ContactMirror::Off => ContactMirror::Off,
            },
        }
    }
}

fn object(o: m::ObjectRef) -> ObjectRef {
    match o {
        m::ObjectRef::Device(c) => ObjectRef::Device(c),
        m::ObjectRef::Wire(c) => ObjectRef::Wire(c),
        m::ObjectRef::Cable(c) => ObjectRef::Cable(c),
        m::ObjectRef::Page(p) => ObjectRef::Page(p),
        m::ObjectRef::Project => ObjectRef::Project,
        m::ObjectRef::Standard => ObjectRef::Standard,
    }
}

impl From<&m::Op> for Op {
    fn from(op: &m::Op) -> Self {
        let ix = |i: usize| i as u64;
        match op {
            m::Op::AddPage { id, page, index } => Op::AddPage { id: *id, page: page.into(), index: ix(*index) },
            m::Op::RemovePage { id, page, index } => Op::RemovePage { id: *id, page: page.into(), index: ix(*index) },
            m::Op::ModifyPage { id, before, after } => Op::ModifyPage { id: *id, before: before.into(), after: after.into() },
            m::Op::AddLayer { id, layer } => Op::AddLayer { id: *id, layer: Layer { name: layer.name.clone() } },
            m::Op::RemoveLayer { id, layer } => Op::RemoveLayer { id: *id, layer: Layer { name: layer.name.clone() } },
            m::Op::AddItem { id, item } => Op::AddItem { id: *id, item: item.into() },
            m::Op::ModifyItem { id, before, after } => Op::ModifyItem { id: *id, before: before.into(), after: after.into() },
            m::Op::RemoveItem { id, item } => Op::RemoveItem { id: *id, item: item.into() },
            m::Op::CreateDevice { id, device } => Op::CreateDevice { id: *id, device: device.into() },
            m::Op::DeleteDevice { id, device } => Op::DeleteDevice { id: *id, device: device.into() },
            m::Op::ModifyDevice { id, before, after } => Op::ModifyDevice { id: *id, before: before.into(), after: after.into() },
            m::Op::SetDeviceDefinition { id, before, after } => Op::SetDeviceDefinition { id: *id, before: *before, after: *after },
            m::Op::AddConnectionPoint { id, connection_point } => Op::AddConnectionPoint { id: *id, connection_point: connection_point.into() },
            m::Op::RemoveConnectionPoint { id, connection_point } => Op::RemoveConnectionPoint { id: *id, connection_point: connection_point.into() },
            m::Op::LinkRepresentation { id, rep } => Op::LinkRepresentation { id: *id, rep: rep.into() },
            m::Op::ModifyRepresentation { id, before, after } => Op::ModifyRepresentation { id: *id, before: before.into(), after: after.into() },
            m::Op::UnlinkRepresentation { id, rep } => Op::UnlinkRepresentation { id: *id, rep: rep.into() },
            m::Op::CreateWire { id, wire } => Op::CreateWire { id: *id, wire: wire.into() },
            m::Op::DeleteWire { id, wire } => Op::DeleteWire { id: *id, wire: wire.into() },
            m::Op::ModifyWire { id, before, after } => Op::ModifyWire { id: *id, before: before.into(), after: after.into() },
            m::Op::AddRoute { id, route } => Op::AddRoute { id: *id, route: route.into() },
            m::Op::RemoveRoute { id, route } => Op::RemoveRoute { id: *id, route: route.into() },
            m::Op::ModifyRoute { id, before, after } => Op::ModifyRoute { id: *id, before: before.into(), after: after.into() },
            m::Op::CreateCable { id, cable } => Op::CreateCable { id: *id, cable: cable.into() },
            m::Op::DeleteCable { id, cable } => Op::DeleteCable { id: *id, cable: cable.into() },
            m::Op::ModifyCable { id, before, after } => Op::ModifyCable { id: *id, before: before.into(), after: after.into() },
            m::Op::SetProperty { object: o, key, before, after } => {
                Op::SetProperty { object: object(*o), key: key.clone(), before: before.clone(), after: after.clone() }
            }
            m::Op::EmbedDefinition { id, definition } => Op::EmbedDefinition { id: *id, definition: definition.into() },
            m::Op::RemoveDefinition { id, definition } => Op::RemoveDefinition { id: *id, definition: definition.into() },
            m::Op::SetProjectSettings { before, after } => Op::SetProjectSettings { before: before.into(), after: after.into() },
            m::Op::NameVersion { name, at, description } => Op::NameVersion { name: name.clone(), at: *at, description: description.clone() },
            m::Op::UnnameVersion { name, at, description } => Op::UnnameVersion { name: name.clone(), at: *at, description: description.clone() },
            m::Op::AddFolder { id, folder, index } => Op::AddFolder { id: *id, folder: folder.into(), index: ix(*index) },
            m::Op::RemoveFolder { id, folder, index } => Op::RemoveFolder { id: *id, folder: folder.into(), index: ix(*index) },
            m::Op::ModifyFolder { id, before, after } => Op::ModifyFolder { id: *id, before: before.into(), after: after.into() },
            m::Op::SetStyle { layer: l, catalogue, id, before, after } => {
                Op::SetStyle { layer: layer(*l), catalogue: *catalogue, id: id.clone(), before: before.clone(), after: after.clone() }
            }
            m::Op::SetStandard { before, after } => Op::SetStandard { before: before.clone(), after: after.clone() },
            m::Op::ModifyProject { before, after } => Op::ModifyProject { before: before.clone(), after: after.clone() },
            m::Op::AddDeviceLink { id, link } => Op::AddDeviceLink { id: *id, link: *link },
            m::Op::RemoveDeviceLink { id, link } => Op::RemoveDeviceLink { id: *id, link: *link },
            m::Op::CreateAid { id, aid } => Op::CreateAid { id: *id, aid: aid.clone() },
            m::Op::DeleteAid { id, aid } => Op::DeleteAid { id: *id, aid: aid.clone() },
            m::Op::ModifyAid { id, before, after } => Op::ModifyAid { id: *id, before: before.clone(), after: after.clone() },
            m::Op::CreateLoosePart { id, part } => Op::CreateLoosePart { id: *id, part: part.clone() },
            m::Op::DeleteLoosePart { id, part } => Op::DeleteLoosePart { id: *id, part: part.clone() },
            m::Op::ModifyLoosePart { id, before, after } => Op::ModifyLoosePart { id: *id, before: before.clone(), after: after.clone() },
        }
    }
}

impl From<&m::Author> for Author {
    fn from(a: &m::Author) -> Self {
        match a {
            m::Author::User => Author::User,
            m::Author::Agent(n) => Author::Agent(n.clone()),
        }
    }
}

impl From<&m::TxKind> for TxKind {
    fn from(k: &m::TxKind) -> Self {
        match *k {
            m::TxKind::Edit => TxKind::Edit,
            m::TxKind::Undo { of } => TxKind::Undo(of),
            m::TxKind::Redo { of } => TxKind::Redo(of),
            m::TxKind::Merge { source, source_head, ancestor } => TxKind::Merge(MergeKind {
                source: Hex(source),
                source_head: MergeStep { seq: source_head.seq, chain: Hex(source_head.chain) },
                ancestor: MergeAncestor { branch: Hex(ancestor.branch), seq: ancestor.seq, chain: Hex(ancestor.chain) },
            }),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// record → model

/// A record value the model cannot hold.
pub(crate) type ConvertError = String;

fn index(i: u64) -> Result<usize, ConvertError> {
    usize::try_from(i).map_err(|_| format!("index {i} is out of range"))
}

impl From<Color> for m::Color {
    fn from(c: Color) -> Self {
        match c {
            Color::ByLayer => m::Color::ByLayer,
            Color::ByBlock => m::Color::ByBlock,
            Color::Index(i) => m::Color::Index(i),
            Color::Rgb(v) => m::Color::Rgb(v),
        }
    }
}

impl From<Primitive> for m::Primitive {
    fn from(p: Primitive) -> Self {
        match p {
            Primitive::Line { a, b } => m::Primitive::Line { a: mpt(a), b: mpt(b) },
            Primitive::Polyline { vertices, closed } => m::Primitive::Polyline { vertices: vertices.into_iter().map(mpt).collect(), closed },
            Primitive::Arc { center, radius, start, end } => m::Primitive::Arc { center: mpt(center), radius, start, end },
            Primitive::Circle { center, radius } => m::Primitive::Circle { center: mpt(center), radius },
            Primitive::Text { at, height, rotation, value, halign, valign } => {
                m::Primitive::Text { at: mpt(at), height, rotation, value, halign, valign }
            }
            Primitive::Point { at } => m::Primitive::Point { at: mpt(at) },
        }
    }
}

impl From<Item> for m::Item {
    fn from(i: Item) -> Self {
        let kind = match i.kind {
            ItemKind::Primitive(p) => m::ItemKind::Primitive(p.into()),
            ItemKind::Symbol(s) => m::ItemKind::Symbol(m::SymbolInstance {
                definition: s.definition,
                placement: m::Placement {
                    origin: mpt(s.placement.origin),
                    rotation: s.placement.rotation,
                    mirror: s.placement.mirror,
                    scale: s.placement.scale,
                },
            }),
        };
        m::Item {
            page: i.page,
            style: m::Style { layer: i.style.layer, color: i.style.color.into(), linetype: i.style.linetype, lineweight: i.style.lineweight },
            kind,
        }
    }
}

impl From<Device> for m::Device {
    fn from(c: Device) -> Self {
        m::Device {
            tag: c.tag,
            description: c.description,
            part_number: c.part_number,
            manufacturer: c.manufacturer,
            article_number: c.article_number,
            type_text: c.type_text,
            location: c.location,
            function: c.function,
            rating_note: c.rating_note,
            properties: c.properties,
            definition: c.definition,
            device_type: c.device_type,
            user: c.user,
        }
    }
}

impl From<ConnectionPoint> for m::ConnectionPoint {
    fn from(t: ConnectionPoint) -> Self {
        m::ConnectionPoint { owner: t.owner, label: t.label, key: t.key, accepts: t.accepts, protective_earth: t.protective_earth }
    }
}

impl From<Representation> for m::Representation {
    fn from(r: Representation) -> Self {
        m::Representation {
            subject: match r.subject {
                Subject::Device(c) => m::Subject::Device(c),
                Subject::Wire(c) => m::Subject::Wire(c),
                Subject::Cable(c) => m::Subject::Cable(c),
                Subject::Route(c) => m::Subject::Route(c),
                Subject::Aid(c) => m::Subject::Aid(c),
            },
            page: r.page,
            form: match r.form {
                Form::Drawn => m::Form::Drawn,
                Form::Symbol => m::Form::Symbol,
            },
            role: r.role,
            items: r.items,
            connection_points: r.connection_points.into_iter().map(|(k, v)| (k, mpt(v))).collect(),
        }
    }
}

impl From<Wire> for m::Wire {
    fn from(c: Wire) -> Self {
        m::Wire {
            net: c.net,
            conductor: c.conductor,
            net_order: c.net_order,
            ends: c.ends,
            order_fixed: c.order_fixed,
            wire_type: c.wire_type,
            size: c.size,
            colours: c.colours,
            number: c.number,
            potential: c.potential,
            rating: c.rating,
            load: c.load,
            protected_by: c.protected_by,
            display: c.display,
        }
    }
}

impl From<Core> for m::Core {
    fn from(c: Core) -> Self {
        m::Core { label: c.label, colours: c.colours, purpose: c.purpose.into(), position: c.position.into(), through: c.through }
    }
}

impl From<Cable> for m::Cable {
    fn from(c: Cable) -> Self {
        m::Cable {
            tag: c.tag,
            description: c.description,
            properties: c.properties,
            cores: c.cores.into_iter().map(Into::into).collect(),
            cable_type: c.cable_type,
            shield: c.shield,
            rating: c.rating,
            load: c.load,
            protected_by: c.protected_by,
        }
    }
}

impl From<Definition> for m::Definition {
    fn from(d: Definition) -> Self {
        m::Definition {
            name: d.name,
            origin: d.origin.map(morigin),
            graphics: d.graphics.into_iter().map(Into::into).collect(),
            connection_points: d
                .connection_points
                .into_iter()
                .map(|t| m::ConnectionPointDef { key: t.key, label: t.label, at: mpt(t.at), accepts: t.accepts })
                .collect(),
            bridges: d.bridges,
            texts: d.texts.into_iter().map(|t| m::TextSlot { text: t.text, at: mpt(t.at), h: t.h, v: t.v, rotation: t.rotation }).collect(),
            defaults: d.defaults,
            derived_from: d.derived_from.map(morigin),
            device_type: d.device_type,
            part_role: d.part_role,
        }
    }
}

impl From<ProjectSettings> for m::ProjectSettings {
    fn from(s: ProjectSettings) -> Self {
        m::ProjectSettings {
            length_unit: match s.length_unit {
                LengthUnit::Millimetre => m::LengthUnit::Millimetre,
                LengthUnit::Inch => m::LengthUnit::Inch,
            },
            tag_padding: s.tag_padding,
            tag_prefixes: s.tag_prefixes,
            cable_prefix: s.cable_prefix,
            wiring: s.wiring.into(),
            sheet: s.sheet.map(Into::into),
            page_number_step: s.page_number_step,
            reference_style: s.reference_style.map(|r| match r {
                ReferenceStyle::PageColumnRow => m::ReferenceStyle::PageColumnRow,
                ReferenceStyle::PageColumn => m::ReferenceStyle::PageColumn,
                ReferenceStyle::PageZone => m::ReferenceStyle::PageZone,
            }),
            contact_mirror: match s.contact_mirror {
                ContactMirror::Auto => m::ContactMirror::Auto,
                ContactMirror::Below => m::ContactMirror::Below,
                ContactMirror::Right => m::ContactMirror::Right,
                ContactMirror::Off => m::ContactMirror::Off,
            },
        }
    }
}

fn mobject(o: ObjectRef) -> m::ObjectRef {
    match o {
        ObjectRef::Device(c) => m::ObjectRef::Device(c),
        ObjectRef::Wire(c) => m::ObjectRef::Wire(c),
        ObjectRef::Cable(c) => m::ObjectRef::Cable(c),
        ObjectRef::Page(p) => m::ObjectRef::Page(p),
        ObjectRef::Project => m::ObjectRef::Project,
        ObjectRef::Standard => m::ObjectRef::Standard,
    }
}

impl TryFrom<Op> for m::Op {
    type Error = ConvertError;
    fn try_from(op: Op) -> Result<Self, ConvertError> {
        Ok(match op {
            Op::AddPage { id, page, index: i } => m::Op::AddPage { id, page: page.into(), index: index(i)? },
            Op::RemovePage { id, page, index: i } => m::Op::RemovePage { id, page: page.into(), index: index(i)? },
            Op::ModifyPage { id, before, after } => m::Op::ModifyPage { id, before: before.into(), after: after.into() },
            Op::AddLayer { id, layer } => m::Op::AddLayer { id, layer: m::Layer { name: layer.name } },
            Op::RemoveLayer { id, layer } => m::Op::RemoveLayer { id, layer: m::Layer { name: layer.name } },
            Op::AddItem { id, item } => m::Op::AddItem { id, item: item.into() },
            Op::ModifyItem { id, before, after } => m::Op::ModifyItem { id, before: before.into(), after: after.into() },
            Op::RemoveItem { id, item } => m::Op::RemoveItem { id, item: item.into() },
            Op::CreateDevice { id, device } => m::Op::CreateDevice { id, device: device.into() },
            Op::DeleteDevice { id, device } => m::Op::DeleteDevice { id, device: device.into() },
            Op::ModifyDevice { id, before, after } => m::Op::ModifyDevice { id, before: before.into(), after: after.into() },
            Op::SetDeviceDefinition { id, before, after } => m::Op::SetDeviceDefinition { id, before, after },
            Op::AddConnectionPoint { id, connection_point } => m::Op::AddConnectionPoint { id, connection_point: connection_point.into() },
            Op::RemoveConnectionPoint { id, connection_point } => m::Op::RemoveConnectionPoint { id, connection_point: connection_point.into() },
            Op::LinkRepresentation { id, rep } => m::Op::LinkRepresentation { id, rep: rep.into() },
            Op::ModifyRepresentation { id, before, after } => m::Op::ModifyRepresentation { id, before: before.into(), after: after.into() },
            Op::UnlinkRepresentation { id, rep } => m::Op::UnlinkRepresentation { id, rep: rep.into() },
            Op::CreateWire { id, wire } => m::Op::CreateWire { id, wire: wire.into() },
            Op::DeleteWire { id, wire } => m::Op::DeleteWire { id, wire: wire.into() },
            Op::ModifyWire { id, before, after } => m::Op::ModifyWire { id, before: before.into(), after: after.into() },
            Op::AddRoute { id, route } => m::Op::AddRoute { id, route: route.into() },
            Op::RemoveRoute { id, route } => m::Op::RemoveRoute { id, route: route.into() },
            Op::ModifyRoute { id, before, after } => m::Op::ModifyRoute { id, before: before.into(), after: after.into() },
            Op::CreateCable { id, cable } => m::Op::CreateCable { id, cable: cable.into() },
            Op::DeleteCable { id, cable } => m::Op::DeleteCable { id, cable: cable.into() },
            Op::ModifyCable { id, before, after } => m::Op::ModifyCable { id, before: before.into(), after: after.into() },
            Op::SetProperty { object, key, before, after } => m::Op::SetProperty { object: mobject(object), key, before, after },
            Op::EmbedDefinition { id, definition } => m::Op::EmbedDefinition { id, definition: definition.into() },
            Op::RemoveDefinition { id, definition } => m::Op::RemoveDefinition { id, definition: definition.into() },
            Op::SetProjectSettings { before, after } => m::Op::SetProjectSettings { before: before.into(), after: after.into() },
            Op::NameVersion { name, at, description } => m::Op::NameVersion { name, at, description },
            Op::UnnameVersion { name, at, description } => m::Op::UnnameVersion { name, at, description },
            Op::AddFolder { id, folder, index: i } => m::Op::AddFolder { id, folder: folder.into(), index: index(i)? },
            Op::RemoveFolder { id, folder, index: i } => m::Op::RemoveFolder { id, folder: folder.into(), index: index(i)? },
            Op::ModifyFolder { id, before, after } => m::Op::ModifyFolder { id, before: before.into(), after: after.into() },
            Op::SetStyle { layer: l, catalogue, id, before, after } => m::Op::SetStyle { layer: mlayer(l), catalogue, id, before, after },
            Op::SetStandard { before, after } => m::Op::SetStandard { before, after },
            Op::ModifyProject { before, after } => m::Op::ModifyProject { before, after },
            Op::AddDeviceLink { id, link } => m::Op::AddDeviceLink { id, link },
            Op::RemoveDeviceLink { id, link } => m::Op::RemoveDeviceLink { id, link },
            Op::CreateAid { id, aid } => m::Op::CreateAid { id, aid },
            Op::DeleteAid { id, aid } => m::Op::DeleteAid { id, aid },
            Op::ModifyAid { id, before, after } => m::Op::ModifyAid { id, before, after },
            Op::CreateLoosePart { id, part } => m::Op::CreateLoosePart { id, part },
            Op::DeleteLoosePart { id, part } => m::Op::DeleteLoosePart { id, part },
            Op::ModifyLoosePart { id, before, after } => m::Op::ModifyLoosePart { id, before, after },
        })
    }
}

impl From<Author> for m::Author {
    fn from(a: Author) -> Self {
        match a {
            Author::User => m::Author::User,
            Author::Agent(n) => m::Author::Agent(n),
        }
    }
}

impl From<TxKind> for m::TxKind {
    fn from(k: TxKind) -> Self {
        match k {
            TxKind::Edit => m::TxKind::Edit,
            TxKind::Undo(of) => m::TxKind::Undo { of },
            TxKind::Redo(of) => m::TxKind::Redo { of },
            TxKind::Merge(k) => m::TxKind::Merge {
                source: k.source.0,
                source_head: m::StepPoint { seq: k.source_head.seq, chain: k.source_head.chain.0 },
                ancestor: m::BranchPoint { branch: k.ancestor.branch.0, seq: k.ancestor.seq, chain: k.ancestor.chain.0 },
            },
        }
    }
}
