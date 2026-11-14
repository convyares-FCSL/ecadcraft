//! The Checks list (brief 022, design 022 §8): open ends, loose wires, false-looking connections,
//! nets with one connection point, unconnected device points, number and potential conflicts.
//!
//! # Semantics
//! - Every check is derived from the document; nothing is stored.
//! - Checks belong to a **subject**: a net, a conductor, a connection point, a component, or the
//!   project (the unnumbered-nets row). [`Checks`] keeps the checks by subject and is built whole
//!   only on open, reload or first show ([`Checks::build`]); after that [`Checks::update`] re-checks
//!   only the subjects a transaction's ops could change (the update map below). The geometric
//!   checks use a per-page grid of route segments and loose ends ([`GRID_MM`] cells), built with
//!   the list and kept by the same updates.
//!
//! # Update map (design 022 §11)
//! | Op | Re-checked |
//! | --- | --- |
//! | `CreateConductor` / `DeleteConductor` / `AttachEnd` | that conductor; the connection points at its ends before and after; their nets (before and after) and the nets sharing their numbers |
//! | `AddTerminal` / `RemoveTerminal` | that point and its net |
//! | `CreateComponent` / `DeleteComponent` / `SetProperty` component | that component (its contact count against its part's `contacts`, brief 025); its points' nets; continuations with its name (partners) |
//! | `SetProperty` conductor | that conductor's net, and the nets sharing its old and new number |
//! | `AddItem` / `ModifyItem` / `RemoveItem` / representation ops of a route | that conductor; loose ends near its old and new route |
//! | the same of a component symbol | conductors attached to its points ("drawn away"); continuations with its name |
//! | `EmbedDefinition` / `RemoveDefinition` | components using it and their points |
//! | `SetProjectSettings` changing the numbering or potentials (`wiring` scheme, digits, prefix, table) | every net (rare and explicit) |
//! | `SetProjectSettings` changing anything else (label looks, tags, sheet, reference style) | nothing (sheet and reference style: rows are copied again) |
//! | representation ops of a cable (reference ends), `CreateCable` / `DeleteCable` / cable ops | that cable's reference ends (orphan, brief 027) |
//! | `RemovePage` | the cables with a reference end on it (orphan, brief 027) |
//! | pages, layers, versions | nothing (page texts are worked out when shown) |
//!
//! This module reads the document; it changes nothing.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ecad_model::connectivity::{CONTINUATION, JUNCTION, NAME, name_key};
use ecad_model::wiring;
use ecad_model::{CableId, ComponentId, ConductorId, Document, End, NetData, NetId, ObjectRef, Op, PageId, Pt, Subject, TerminalId};
use serde_json::{Value, json};

use crate::wires::{self, ON_WIRE, kind_of};

/// The grid cell size of the geometric checks, in millimetres.
pub const GRID_MM: f64 = 10.0;
/// Most rows `checks` returns per call by default, and at most.
pub const DEFAULT_LIMIT: usize = 200;
pub const MAX_LIMIT: usize = 2000;

/// How serious a check is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }

    /// The glyph shown before a row.
    pub fn glyph(self) -> &'static str {
        match self {
            Severity::Error => "\u{25cf}",
            Severity::Warning => "\u{25b2}",
            Severity::Info => "\u{b7}",
        }
    }
}

/// What a check found (design 022 §8's table, in its order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    JoinsPotentials,
    SharedNumber,
    TwoFixed,
    OpenEnd,
    LooseWire,
    LooksConnected,
    DrawnAway,
    SinglePoint,
    ThinJunction,
    Unconnected,
    LonelyContinuation,
    MixedPotentials,
    Unnumbered,
    UnknownName,
    /// Two cables, or a cable and a device, share a tag (brief 026: one namespace).
    DuplicateTag,
    /// A provisional tag (brief 026, design §5.4).
    ProvisionalTag,
    /// Nets without a number while provisional tags are on: one summary row (Q6).
    ProvisionalWires,
    /// Brief 024 (design 024 §8.3): a device's own location differs from its box's.
    OwnLocation,
    /// A location box outline crosses itself.
    BoxCrosses,
    /// Two location boxes overlap without nesting.
    BoxesOverlap,
    /// Devices without a part number (one count row).
    NoPartNumber,
    /// Devices without a location (one count row).
    NoLocation,
    /// A device with more contacts of a kind drawn than its part offers (brief 025).
    TooManyContacts,
    /// Brief 028 (design 028 §8): a strip with more terminals drawn than it offers.
    TooManyTerminals,
    /// Two terminals of one strip with one label.
    TerminalTwice,
    /// A cable with more members than cores offered.
    TooManyCores,
    /// Two members of one cable with one core label.
    CoreTwice,
    /// Cables with no core count (one count row).
    NoCoreCount,
    /// A cable reference end with no other end (brief 027, design 027 §4.2).
    OrphanReference,
    /// Brief 030 (design 030 §8): an accessory placed after or on a position the strip lacks.
    AccessoryOrphan,
    /// Placed strip accessories without a part number (one count row).
    NoAccessoryPart,
    /// A link spanning a partition.
    LinkAcrossPartition,
    /// A link naming positions the strip does not have.
    LinkOrphan,
    /// A `link.<n>` key on a device that is not a terminal strip (review of PR #57, 4).
    LinkOnNonStrip,
    /// Brief 033: a contact table the user placed overlaps the title block.
    TableOverTitle,
    /// Brief 033 (design 033 §5.3): a generated page grew onto another page's number.
    ContinuationClash,
    /// Brief 036 (design 036 §8): a level of a multi-level position drawn twice.
    LevelTwice,
    /// A position whose levels are drawn by symbols of two types.
    PositionMixed,
    /// A link naming a level its position does not have.
    LevelMissing,
    /// Fuse terminals that are devices of their own (one count row).
    FuseDevice,
    /// Fuse positions with no rating (one count row).
    NoFuseRating,
    /// Brief 040: a wire or core colour code the app does not know (drawn in ink).
    UnknownColour,
    /// Brief 040: a page with more two-colour dashes than it draws (one row per page).
    DashesLeftOut,
    /// Brief 045 (design 045 §9): two cards in one slot of a rack.
    SlotTwice,
    /// A card naming a rack that is not in the project.
    RackMissing,
    /// A card in a slot its rack does not have (or in a rack but no slot).
    SlotRange,
    /// PLC cards in no rack (one count row).
    Unracked,
    /// Brief 045b: one channel drawn twice.
    ChannelTwice,
    /// A channel drawn beyond its card's channel count.
    ChannelOver,
    /// A channel drawn by a symbol of another card type.
    ChannelType,
    /// Two cards' address blocks overlap.
    AddressOverlap,
    /// One address given to two channels (one typed).
    AddressTwice,
    /// A block past byte 65 535.
    AddressOverflow,
    /// Used channels with no address (one count row).
    NoAddress,
    /// Brief 044 (design 044 §6.3): a connector pin drawn twice on one half.
    ConnectorPinTwice,
    /// A connector whose whole halves are of two families.
    ConnectorFamily,
    /// A connector whose whole halves differ in size.
    ConnectorSize,
    /// A connector's pins with no mate (one count row per connector).
    ConnectorLonePin,
    /// A connector pin numbered beyond its `pins`.
    ConnectorOver,
    /// Brief 047 (design 047 §9): items without a part number (one count row).
    ItemNoPartNumber,
    /// Fuses (of banks, strips and fuse terminals) that no part line serves (one count row).
    FuseNoInsert,
    /// One part number with two article numbers.
    ArticleConflict,
    /// A part line's `for` names a fuse or position the device does not have.
    PartForMissing,
    /// A `part.` key, or an item's `qty` or `unit`, ECAD does not read.
    BadPartKey,
    /// Brief 050a: an object's style choice names no style (drawn with the next level's).
    StyleUnknown,
    /// Brief 050a (FP4 ruling): 031's text sizes, weights or fonts set as project properties in
    /// an older build, no longer read. One row per kind of key.
    IgnoredOldSetting,
    /// Brief 050a (design 050 §3.2): style records past the bounds, ignored.
    StyleLimits,
    /// Brief 050b: a wire's own category names no category (drawn without one).
    CategoryUnknown,
    /// Brief 050b: two categories name one potential (the first by ID is used).
    CategoryPotentialTwice,
    /// Brief 050b (design 050 §5.6): a category with a part number has no allowance: its BOM row
    /// has no length.
    WireAllowance,
}

impl Kind {
    pub const ALL: [Kind; 70] = [
        Kind::JoinsPotentials,
        Kind::SharedNumber,
        Kind::TwoFixed,
        Kind::OpenEnd,
        Kind::LooseWire,
        Kind::LooksConnected,
        Kind::DrawnAway,
        Kind::SinglePoint,
        Kind::ThinJunction,
        Kind::Unconnected,
        Kind::LonelyContinuation,
        Kind::MixedPotentials,
        Kind::Unnumbered,
        Kind::UnknownName,
        Kind::DuplicateTag,
        Kind::ProvisionalTag,
        Kind::ProvisionalWires,
        Kind::OwnLocation,
        Kind::BoxCrosses,
        Kind::BoxesOverlap,
        Kind::NoPartNumber,
        Kind::NoLocation,
        Kind::TooManyContacts,
        Kind::TooManyTerminals,
        Kind::TerminalTwice,
        Kind::TooManyCores,
        Kind::CoreTwice,
        Kind::NoCoreCount,
        Kind::OrphanReference,
        Kind::AccessoryOrphan,
        Kind::NoAccessoryPart,
        Kind::LinkAcrossPartition,
        Kind::LinkOrphan,
        Kind::LinkOnNonStrip,
        Kind::TableOverTitle,
        Kind::ContinuationClash,
        Kind::LevelTwice,
        Kind::PositionMixed,
        Kind::LevelMissing,
        Kind::FuseDevice,
        Kind::NoFuseRating,
        Kind::UnknownColour,
        Kind::DashesLeftOut,
        Kind::SlotTwice,
        Kind::RackMissing,
        Kind::SlotRange,
        Kind::Unracked,
        Kind::ChannelTwice,
        Kind::ChannelOver,
        Kind::ChannelType,
        Kind::AddressOverlap,
        Kind::AddressTwice,
        Kind::AddressOverflow,
        Kind::NoAddress,
        Kind::ConnectorPinTwice,
        Kind::ConnectorFamily,
        Kind::ConnectorSize,
        Kind::ConnectorLonePin,
        Kind::ConnectorOver,
        Kind::ItemNoPartNumber,
        Kind::FuseNoInsert,
        Kind::ArticleConflict,
        Kind::PartForMissing,
        Kind::BadPartKey,
        Kind::StyleUnknown,
        Kind::IgnoredOldSetting,
        Kind::StyleLimits,
        Kind::CategoryUnknown,
        Kind::CategoryPotentialTwice,
        Kind::WireAllowance,
    ];

    /// The JSON name.
    pub fn name(self) -> &'static str {
        match self {
            Kind::JoinsPotentials => "joins-potentials",
            Kind::SharedNumber => "shared-number",
            Kind::TwoFixed => "two-fixed",
            Kind::OpenEnd => "open-end",
            Kind::LooseWire => "loose-wire",
            Kind::LooksConnected => "looks-connected",
            Kind::DrawnAway => "drawn-away",
            Kind::SinglePoint => "single-point",
            Kind::ThinJunction => "thin-junction",
            Kind::Unconnected => "unconnected",
            Kind::LonelyContinuation => "lonely-continuation",
            Kind::MixedPotentials => "mixed-potentials",
            Kind::Unnumbered => "unnumbered",
            Kind::UnknownName => "unknown-name",
            Kind::DuplicateTag => "duplicate-tag",
            Kind::ProvisionalTag => "provisional-tag",
            Kind::ProvisionalWires => "provisional-wires",
            Kind::OwnLocation => "own-location",
            Kind::BoxCrosses => "box-crosses",
            Kind::BoxesOverlap => "boxes-overlap",
            Kind::NoPartNumber => "no-part-number",
            Kind::NoLocation => "no-location",
            Kind::TooManyContacts => "too-many-contacts",
            Kind::TooManyTerminals => "too-many-terminals",
            Kind::TerminalTwice => "terminal-twice",
            Kind::TooManyCores => "too-many-cores",
            Kind::CoreTwice => "core-twice",
            Kind::NoCoreCount => "no-core-count",
            Kind::OrphanReference => "orphan-reference",
            Kind::AccessoryOrphan => "accessory-orphan",
            Kind::NoAccessoryPart => "no-accessory-part",
            Kind::LinkAcrossPartition => "link-across-partition",
            Kind::LinkOrphan => "link-orphan",
            Kind::LinkOnNonStrip => "link-on-non-strip",
            Kind::TableOverTitle => "table-over-title",
            Kind::ContinuationClash => "continuation-clash",
            Kind::LevelTwice => "level-twice",
            Kind::PositionMixed => "position-mixed",
            Kind::LevelMissing => "level-missing",
            Kind::FuseDevice => "fuse-device",
            Kind::NoFuseRating => "no-fuse-rating",
            Kind::UnknownColour => "unknown-colour",
            Kind::DashesLeftOut => "dashes-left-out",
            Kind::SlotTwice => "slot-twice",
            Kind::RackMissing => "rack-missing",
            Kind::SlotRange => "slot-range",
            Kind::Unracked => "unracked",
            Kind::ChannelTwice => "channel-twice",
            Kind::ChannelOver => "channel-over",
            Kind::ChannelType => "channel-type",
            Kind::AddressOverlap => "address-overlap",
            Kind::AddressTwice => "address-twice",
            Kind::AddressOverflow => "address-overflow",
            Kind::NoAddress => "no-address",
            Kind::ConnectorPinTwice => "connector-pin-twice",
            Kind::ConnectorFamily => "connector-family",
            Kind::ConnectorSize => "connector-size",
            Kind::ConnectorLonePin => "connector-lone-pin",
            Kind::ConnectorOver => "connector-over",
            Kind::ItemNoPartNumber => "item-no-part-number",
            Kind::FuseNoInsert => "fuse-no-insert",
            Kind::ArticleConflict => "article-conflict",
            Kind::PartForMissing => "part-for-missing",
            Kind::BadPartKey => "bad-part-key",
            Kind::StyleUnknown => "style-unknown",
            Kind::IgnoredOldSetting => "ignored-old-setting",
            Kind::StyleLimits => "style-limits",
            Kind::CategoryUnknown => "category-unknown",
            Kind::CategoryPotentialTwice => "category-potential-twice",
            Kind::WireAllowance => "wire-allowance",
        }
    }

    /// The kind a JSON name gives.
    pub fn parse(text: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == text.trim())
    }

    /// The words of the Show list.
    pub fn words(self) -> &'static str {
        match self {
            Kind::JoinsPotentials => "Net joins two potentials",
            Kind::SharedNumber => "Two nets share a number",
            Kind::TwoFixed => "One net, two fixed numbers",
            Kind::OpenEnd => "Open end",
            Kind::LooseWire => "Loose wire",
            Kind::LooksConnected => looks_connected_words(),
            Kind::DrawnAway => "End drawn away from its connection point",
            Kind::SinglePoint => "Net with a single connection point",
            Kind::ThinJunction => "Junction with fewer than three wires",
            Kind::Unconnected => "Connection point not connected",
            Kind::LonelyContinuation => "Continuation without partner",
            Kind::MixedPotentials => "Mixed potentials in one net",
            Kind::Unnumbered => "Unnumbered nets",
            Kind::UnknownName => "Continuation name not a potential",
            Kind::DuplicateTag => "Two cables share a tag",
            Kind::ProvisionalTag => "Provisional tag",
            Kind::ProvisionalWires => "Wire numbers provisional",
            Kind::OwnLocation => "Own location differs from its box",
            Kind::BoxCrosses => "Location box outline crosses itself",
            Kind::BoxesOverlap => "Location boxes overlap without nesting",
            Kind::NoPartNumber => "Devices without a part number",
            Kind::NoLocation => "Devices without a location",
            Kind::TooManyContacts => "More contacts than the part offers",
            Kind::TooManyTerminals => "More terminals than the strip offers",
            Kind::TerminalTwice => "Terminal drawn twice",
            Kind::TooManyCores => "More cores used than the cable offers",
            Kind::CoreTwice => "Core used by two wires",
            Kind::NoCoreCount => "Cables without a core count",
            Kind::OrphanReference => "Cable reference with no other end",
            Kind::AccessoryOrphan => "Accessory at a position the strip lacks",
            Kind::NoAccessoryPart => "Strip accessories without a part number",
            Kind::LinkAcrossPartition => "Link crosses a partition",
            Kind::LinkOrphan => "Link names positions the strip lacks",
            Kind::LinkOnNonStrip => "Link on a device that is not a strip",
            Kind::TableOverTitle => "Contact table over the title block",
            Kind::ContinuationClash => "Continuation sheet has another page's number",
            Kind::LevelTwice => "Terminal level drawn twice",
            Kind::PositionMixed => "Position mixes terminal types",
            Kind::LevelMissing => "Link names a level the position lacks",
            Kind::FuseDevice => "Fuse terminals that are devices of their own",
            Kind::NoFuseRating => "Fuse positions without a rating",
            Kind::UnknownColour => "Colour code not defined (drawn in ink); define it in Standards",
            Kind::DashesLeftOut => "Two-colour wires drawn in one colour (too many dashes on the page)",
            Kind::SlotTwice => "Two cards in one slot",
            Kind::RackMissing => "Card names a rack that is not in the project",
            Kind::SlotRange => "Card in a slot its rack does not have",
            Kind::Unracked => "PLC cards in no rack",
            Kind::ChannelTwice => "Channel drawn twice",
            Kind::ChannelOver => "Channel beyond the card's count",
            Kind::ChannelType => "Channel drawn as another card type",
            Kind::AddressOverlap => "Address blocks overlap",
            Kind::AddressTwice => "Address given twice",
            Kind::AddressOverflow => "Address block past byte 65535",
            Kind::NoAddress => "Used channels with no address",
            Kind::ConnectorPinTwice => "Connector pin drawn twice on one half",
            Kind::ConnectorFamily => "Connector halves of two families",
            Kind::ConnectorSize => "Connector halves of two sizes",
            Kind::ConnectorLonePin => "Connector pins without a mate",
            Kind::ConnectorOver => "Connector pin beyond its pin count",
            Kind::ItemNoPartNumber => "Items without a part number",
            Kind::FuseNoInsert => "Fuses without an insert",
            Kind::ArticleConflict => "One part number, two article numbers",
            Kind::PartForMissing => "Part line names a fuse or position the device lacks",
            Kind::BadPartKey => "Part line or item value ECAD does not read",
            Kind::StyleUnknown => "Style not found; drawn without it",
            Kind::IgnoredOldSetting => "Set in an older build, now ignored: set it in Standards",
            Kind::StyleLimits => "Style records past the limits are ignored",
            Kind::CategoryUnknown => "Wire category not found; drawn without one",
            Kind::CategoryPotentialTwice => "Two wire categories are the default for one potential",
            Kind::WireAllowance => "Wire length not set (set an allowance in Standards)",
        }
    }

    pub fn severity(self) -> Severity {
        match self {
            Kind::JoinsPotentials
            | Kind::SharedNumber
            | Kind::TwoFixed
            | Kind::DuplicateTag
            | Kind::TerminalTwice
            | Kind::LevelTwice
            | Kind::SlotTwice
            | Kind::ChannelTwice
            | Kind::ConnectorPinTwice => Severity::Error,
            Kind::OpenEnd
            | Kind::LooseWire
            | Kind::LooksConnected
            | Kind::DrawnAway
            | Kind::SinglePoint
            | Kind::UnknownName
            | Kind::ProvisionalTag
            | Kind::ProvisionalWires
            | Kind::TooManyContacts
            | Kind::TooManyTerminals
            | Kind::TooManyCores
            | Kind::CoreTwice
            | Kind::OwnLocation
            | Kind::BoxCrosses
            | Kind::OrphanReference
            | Kind::AccessoryOrphan
            | Kind::LinkAcrossPartition
            | Kind::LinkOrphan
            | Kind::LinkOnNonStrip
            | Kind::TableOverTitle
            | Kind::ContinuationClash
            | Kind::PositionMixed
            | Kind::LevelMissing
            | Kind::DashesLeftOut
            | Kind::RackMissing
            | Kind::SlotRange
            | Kind::ChannelOver
            | Kind::ChannelType
            | Kind::AddressOverlap
            | Kind::AddressTwice
            | Kind::AddressOverflow
            | Kind::ConnectorFamily
            | Kind::ConnectorSize
            | Kind::ArticleConflict
            | Kind::PartForMissing
            | Kind::CategoryPotentialTwice => Severity::Warning,
            _ => Severity::Info,
        }
    }
}

/// What a row points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Object {
    Conductor(ConductorId),
    Terminal(TerminalId),
    Component(ComponentId),
    /// A cable reference end (brief 027).
    Reference(ecad_model::RepresentationId),
    Cable(ecad_model::CableId),
    None,
}

/// What a fix button on a row does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fix {
    /// `junction {at}` on the row's page.
    AddJunction(Pt),
    /// 032: `joinwires {at}` on the row's page (Looks connected: **Join**).
    Join(Pt),
    /// `renumberwires {scope: "unnumbered"}`.
    NumberThem,
    /// Open Finalise numbering (brief 026, design §5.4): the Renumber dialog, not a direct write.
    Finalise,
    /// Brief 024: clear the device's own location (`setprop … value: null`): Use the box's location.
    UseBoxLocation(ComponentId),
    /// Brief 024: open Reports on the BOM tab with Only ? on.
    ShowReports,
    /// Brief 028: offer the drawn count (`setprop terminals`, PE kept): Offer 14.
    OfferDrawn(ComponentId),
    /// Brief 028: renumber the strip's terminals (`renumber {kind: "terminals"}`).
    RenumberStrip(ComponentId),
    /// Brief 028: focus Properties ▸ Cores on the cable.
    SetCores(ecad_model::CableId),
    /// Brief 028: give the member the next free core (`cablecore`).
    NextFreeCore(ConductorId),
    /// Brief 028: open Reports on the Cables tab with Only ? on.
    ShowCables,
    /// Brief 030: the strip's orphan key (index into its kept orphans): **Move to end** (items
    /// appended to `acc.end`, key cleared) or **Remove** (key cleared).
    AccessoryOrphan(ComponentId, u16),
    /// Brief 030: link `n` crossing the partition after position index `at`: **Split link at**
    /// (`jumper split_at`) or **Remove partition** (`accessory remove [partition]`).
    LinkAcrossPartition(ComponentId, u16, u16),
    /// Brief 030: link `n` names missing positions: **Drop missing positions** or **Delete link**.
    LinkOrphan(ComponentId, u16),
    /// Brief 030 (review of PR #57, 4): **Remove** a link key from a device that is not a strip.
    LinkOnNonStrip(ComponentId),
    /// Brief 033 (design 033 §4.1): **Move table…** starts `mirrormove` for the device.
    MoveTable(ComponentId),
    /// Brief 033 (design 033 §5.3): **Renumber…** opens this page's number field.
    RenumberPage(PageId),
    /// Brief 036: **Move to next free** (the level drawn twice, to the next position of its type
    /// with that level free; `termlabel {terminal, move_to: "next"}`).
    MoveToNextFree(TerminalId),
    /// Brief 036: **Move to new position** (the other type's levels; `termlabel {terminal,
    /// move_to: "new"}`).
    MoveToNewPosition(TerminalId),
    /// Brief 036: link `n` names missing levels: **Drop missing levels** or **Delete link**.
    LevelMissing(ComponentId, u16),
    /// Brief 036: **Show** the fuse terminal devices, each with **Make strip position**
    /// (`toposition`).
    FuseDevices,
    /// Brief 045 (design 045 §9): a PLC row's buttons, from the kept PLC model
    /// ([`crate::plc::Plc::fix_commands`]).
    Plc(ComponentId, crate::plc::PlcFix),
    /// Brief 044: **Move to next free** (`pinlabel {terminal, to: "next"}`).
    ConnectorNextPin(TerminalId),
    /// Brief 044: **Set pins to n** (`setprop pins`).
    SetPins(ComponentId, u32),
    /// Brief 044: **Place mates**: Place ▸ Connector on this connector, pin by pin, on the side
    /// the lone pins lack (§4.4 gives them first).
    PlaceMates(ComponentId, crate::symbols::Side),
    /// Brief 044: **Ignore** this warning on this connector (`setprop ignore.<kind> yes`).
    IgnoreConnector(ComponentId, Kind),
    /// Brief 047: **Use A-0110 for all**: the most common article number of the part of line
    /// `n` of the component (`article`).
    UseArticle(ComponentId, u8),
    /// Brief 047: **Remove 9**: drop the names line `n`'s `for` lists that the device lacks
    /// (`partline set for`).
    DropMissingFor(ComponentId, u8),
    /// Brief 050a: **Go to** Settings ▸ Standards at this catalogue (`text`, `wire`…).
    OpenStandards(&'static str),
}

/// One row of the Checks list.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub kind: Kind,
    pub page: Option<PageId>,
    pub at: Option<Pt>,
    pub object: Object,
    /// Which net, wire or point, in words (`24V-01`, `-W01 3`, `-M01:PE`).
    pub which: String,
    pub fix: Option<Fix>,
}

impl Check {
    pub fn severity(&self) -> Severity {
        self.kind.severity()
    }

    /// The row's text: the kind's words, or for the unnumbered row its count. Brief 044 (design
    /// 044 §6.3): a connector row's text names its connector (`-X10's halves differ: …`).
    pub fn what(&self) -> String {
        match self.kind {
            Kind::ConnectorPinTwice
            | Kind::ConnectorFamily
            | Kind::ConnectorSize
            | Kind::ConnectorLonePin
            | Kind::ConnectorOver
            | Kind::StyleUnknown
            | Kind::IgnoredOldSetting
            | Kind::StyleLimits
            | Kind::CategoryPotentialTwice
            | Kind::WireAllowance
                if !self.which.is_empty() =>
            {
                self.which.clone()
            }
            _ => self.kind.words().to_string(),
        }
    }

    /// Where the row is, as references name a place (`3-D-5`, or `/3.D` in that style); the page
    /// number alone when it has no point; empty when it has no page.
    pub fn locator(&self, doc: &Document) -> String {
        let Some(page) = self.page else { return String::new() };
        match self.at.and_then(|a| ecad_model::wiring::place(doc, page, a)) {
            Some(p) => p.text(doc.settings.reference_style),
            None => ecad_model::book::page_number(doc, page),
        }
    }

    /// Book order: severity, then page, then position (top to bottom, left to right).
    pub fn order_key(&self, order: &HashMap<PageId, usize>) -> (Severity, usize, i64, i64, Kind, Object) {
        let page = self.page.and_then(|p| order.get(&p).copied()).unwrap_or(usize::MAX);
        let (y, x) = self.at.map_or((0, 0), |a| ((-a.y * 1000.0) as i64, (a.x * 1000.0) as i64));
        (self.severity(), page, y, x, self.kind, self.object)
    }

    pub fn to_json(&self, doc: &Document) -> Value {
        let object = match self.object {
            Object::Conductor(c) => json!({"conductor": c.hex()}),
            Object::Terminal(t) => json!({"terminal": t.hex()}),
            Object::Component(c) => json!({"component": c.hex()}),
            Object::Reference(r) => json!({"reference": r.hex()}),
            Object::Cable(k) => json!({"cable": k.hex()}),
            Object::None => Value::Null,
        };
        let place = self.page.zip(self.at).and_then(|(p, a)| ecad_model::wiring::place(doc, p, a));
        json!({
            "kind": self.kind.name(),
            "severity": self.severity().name(),
            "page": self.page.map(|p| ecad_model::book::page_number(doc, p)),
            "column": place.as_ref().map(|p| p.column_label.clone()),
            "row": place.as_ref().map(|p| p.row_label.clone()),
            "at": self.at.map(|a| [a.x, a.y]),
            "object": object,
            "text": self.what(),
            "which": self.which,
            "where": self.locator(doc),
        })
    }
}

/// What a check belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    Net(NetId),
    Conductor(ConductorId),
    Terminal(TerminalId),
    Component(ComponentId),
    /// The reference ends of a cable (brief 027: orphans).
    CableReferences(CableId),
    Cable(ecad_model::CableId),
    Project,
    /// Brief 040: a page's dash budget.
    Page(PageId),
    /// Brief 050a: the style rows (unknown styles, old settings, limits).
    Styles,
}

type Cell = (PageId, i64, i64);

fn cell_of(page: PageId, p: Pt) -> Option<Cell> {
    if !p.is_finite() {
        return None;
    }
    Some((page, (p.x / GRID_MM).floor() as i64, (p.y / GRID_MM).floor() as i64))
}

/// The cells a segment's box (grown by [`ON_WIRE`]) covers; capped, so a hostile huge segment
/// cannot cost more than a page.
fn cells_of_segment(page: PageId, a: Pt, b: Pt) -> Vec<Cell> {
    let (Some(lo), Some(hi)) = (
        cell_of(page, Pt::new(a.x.min(b.x) - ON_WIRE, a.y.min(b.y) - ON_WIRE)),
        cell_of(page, Pt::new(a.x.max(b.x) + ON_WIRE, a.y.max(b.y) + ON_WIRE)),
    ) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for x in lo.1..=hi.1 {
        for y in lo.2..=hi.2 {
            out.push((page, x, y));
            if out.len() > 10_000 {
                return out;
            }
        }
    }
    out
}

/// The per-page grid of route segments and loose ends.
#[derive(Debug, Clone, Default)]
pub struct Grid {
    segments: HashMap<Cell, BTreeSet<(ConductorId, usize)>>,
    ends: HashMap<Cell, BTreeSet<(ConductorId, End)>>,
    /// What each conductor put in the grid, to take it out again.
    placed: HashMap<ConductorId, Placed>,
}

#[derive(Debug, Clone, Default)]
struct Placed {
    /// (page, segment index, a, b)
    segments: Vec<(PageId, usize, Pt, Pt)>,
    /// (page, end, point) of loose ends
    ends: Vec<(PageId, End, Pt)>,
    cells: BTreeSet<Cell>,
}

/// The route vertices of `c` on each page it is drawn on.
fn routes_of(doc: &Document, c: ConductorId) -> Vec<(PageId, Vec<Pt>)> {
    let mut out = Vec::new();
    for (_, r) in doc.representations_of(Subject::Conductor(c)) {
        if let Some((_, _, v)) = wires::route_vertices(doc, c, r.page)
            && !out.iter().any(|(p, _)| *p == r.page)
        {
            out.push((r.page, v));
        }
    }
    out
}

impl Grid {
    fn remove(&mut self, c: ConductorId) {
        let Some(p) = self.placed.remove(&c) else { return };
        for (page, i, a, b) in &p.segments {
            for cell in cells_of_segment(*page, *a, *b) {
                if let Some(s) = self.segments.get_mut(&cell) {
                    s.remove(&(c, *i));
                    if s.is_empty() {
                        self.segments.remove(&cell);
                    }
                }
            }
        }
        for (page, end, at) in &p.ends {
            if let Some(cell) = cell_of(*page, *at)
                && let Some(s) = self.ends.get_mut(&cell)
            {
                s.remove(&(c, *end));
                if s.is_empty() {
                    self.ends.remove(&cell);
                }
            }
        }
    }

    fn insert(&mut self, doc: &Document, c: ConductorId) {
        self.remove(c);
        let Some(cond) = doc.conductors.get(&c) else { return };
        let mut placed = Placed::default();
        for (page, v) in routes_of(doc, c) {
            for (i, w) in v.windows(2).enumerate() {
                let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
                for cell in cells_of_segment(page, *a, *b) {
                    self.segments.entry(cell).or_default().insert((c, i));
                    placed.cells.insert(cell);
                }
                placed.segments.push((page, i, *a, *b));
            }
            for (end, at) in [(End::A, v.first().copied()), (End::B, v.last().copied())] {
                let Some(at) = at else { continue };
                if cond.end(end).is_none()
                    && let Some(cell) = cell_of(page, at)
                {
                    self.ends.entry(cell).or_default().insert((c, end));
                    placed.cells.insert(cell);
                    placed.ends.push((page, end, at));
                }
            }
        }
        self.placed.insert(c, placed);
    }

    /// The cells `c` occupies now.
    fn cells(&self, c: ConductorId) -> BTreeSet<Cell> {
        self.placed.get(&c).map(|p| p.cells.clone()).unwrap_or_default()
    }

    /// Conductors with a loose end in any of `cells` (and the 8 cells round each).
    fn loose_near(&self, cells: &BTreeSet<Cell>) -> BTreeSet<ConductorId> {
        let mut out = BTreeSet::new();
        for (page, x, y) in cells {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(s) = self.ends.get(&(*page, x + dx, y + dy)) {
                        out.extend(s.iter().map(|(c, _)| *c));
                    }
                }
            }
        }
        out
    }

    /// Whether `at` on `page` lies on the interior of another conductor's route, or on another
    /// loose end: the conductor it lies on.
    fn under(&self, page: PageId, at: Pt, me: ConductorId) -> Option<(ConductorId, bool)> {
        let cell = cell_of(page, at)?;
        for (c, i) in self.segments.get(&cell).into_iter().flatten() {
            if *c == me {
                continue;
            }
            let Some(p) = self.placed.get(c) else { continue };
            let Some((_, _, a, b)) = p.segments.iter().find(|(pg, j, _, _)| *pg == page && j == i) else { continue };
            if near_segment(at, *a, *b) && !at.near(*a, ON_WIRE) && !at.near(*b, ON_WIRE) {
                return Some((*c, false));
            }
        }
        for (c, end) in self.ends.get(&cell).into_iter().flatten() {
            if *c == me {
                continue;
            }
            let Some(p) = self.placed.get(c) else { continue };
            if p.ends.iter().any(|(pg, e, x)| *pg == page && e == end && x.near(at, ON_WIRE)) {
                return Some((*c, true));
            }
        }
        None
    }
}

fn near_segment(p: Pt, a: Pt, b: Pt) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let q = Pt::new(a.x + t * dx, a.y + t * dy);
    q.is_finite() && q.near(p, ON_WIRE)
}

/// Words for a net: its number, or its first device point.
fn net_words(doc: &Document, net: &NetData) -> String {
    wiring::net_number(doc, net).unwrap_or_else(|| wires::net_points(doc, net).first().cloned().unwrap_or_default())
}

/// Where a conductor is: its first page in book order and the midpoint of its longest segment
/// there, or a given end.
fn conductor_place(doc: &Document, c: ConductorId, end: Option<End>) -> (Option<PageId>, Option<Pt>) {
    let order: HashMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    let routes = routes_of(doc, c);
    let Some((page, v)) = routes.iter().min_by_key(|(p, _)| order.get(p).copied().unwrap_or(usize::MAX)) else {
        return (None, None);
    };
    let at = match end {
        Some(End::A) => v.first().copied(),
        Some(End::B) => v.last().copied(),
        None => wires::longest_segment(v).map(|(a, b)| Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)),
    };
    (Some(*page), at)
}

/// Where a connection point is shown first (book order).
fn terminal_place(doc: &Document, t: TerminalId) -> (Option<PageId>, Option<Pt>) {
    let Some(c) = doc.terminals.get(&t).map(|x| x.component) else { return (None, None) };
    for page in &doc.page_order {
        let shown = doc.representations_of(Subject::Component(c)).any(|(_, r)| r.page == *page);
        if shown && let Some(at) = ecad_model::derive::terminal_position(doc, *page, t) {
            return (Some(*page), Some(at));
        }
    }
    (None, None)
}

/// A conductor's words: its cable and core, or its net's number.
fn conductor_words(doc: &Document, c: ConductorId) -> String {
    if let Some(k) = doc.cables_of(c).first() {
        let core = doc.cables.get(k).and_then(|x| x.members.iter().find(|m| m.conductor == c)).and_then(|m| m.core.clone()).unwrap_or_default();
        return format!("{} {core}", crate::references::cable_words(doc, *k)).trim().to_string();
    }
    let conn = doc.connectivity();
    conn.net_of_conductor(c).and_then(|id| conn.net(id)).map(|n| net_words(doc, n)).unwrap_or_default()
}

/// The net checks of net `id`.
pub fn net_checks(doc: &Document, id: NetId, net: &NetData) -> Vec<Check> {
    let mut out = Vec::new();
    let first = net.first_conductor();
    let (page, at) = first.map_or((None, None), |c| conductor_place(doc, c, None));
    let object = first.map_or(Object::None, Object::Conductor);
    let joined = wiring::joined_potentials(doc, net);
    if !joined.is_empty() {
        // Point at a continuation in it, when it has one.
        let cont = net.terminals.iter().find(|t| doc.terminals.get(t).is_some_and(|x| kind_of(doc, x.component) == Some(CONTINUATION)));
        let (page, at, object) = match cont {
            Some(t) => {
                let (p, a) = terminal_place(doc, *t);
                (p, a, doc.terminals.get(t).map_or(Object::None, |x| Object::Component(x.component)))
            }
            None => (page, at, object),
        };
        out.push(Check { kind: Kind::JoinsPotentials, page, at, object, which: joined.join(" and "), fix: None });
    }
    if net.fixed.len() > 1 {
        out.push(Check { kind: Kind::TwoFixed, page, at, object, which: net.fixed.keys().cloned().collect::<Vec<_>>().join(", "), fix: None });
    }
    if let Some(number) = wiring::net_number(doc, net)
        && wiring::sharing(doc, &number).contains(&id)
    {
        out.push(Check { kind: Kind::SharedNumber, page, at, object, which: number, fix: None });
    }
    // A net with conductors, one connection point and no continuation.
    if !net.conductors.is_empty() && net.terminals.len() == 1 && net.names.is_empty() {
        let t = net.terminals.first().copied();
        let jc = t.and_then(|t| doc.terminals.get(&t)).map(|x| kind_of(doc, x.component));
        if !matches!(jc, Some(Some(JUNCTION | CONTINUATION)))
            && let Some(t) = t
        {
            let (p, a) = terminal_place(doc, t);
            out.push(Check { kind: Kind::SinglePoint, page: p, at: a, object: Object::Terminal(t), which: wires::point_words(doc, t), fix: None });
        }
    }
    if net.potentials.len() > 1 {
        let main = wiring::net_potential(doc, net).name;
        let odd = net
            .conductors
            .iter()
            .find(|c| doc.conductors.get(c).and_then(|x| x.properties.get(wiring::POTENTIAL_KEY)).is_some_and(|p| Some(p) != main.as_ref()));
        let (p, a) = odd.map_or((page, at), |c| conductor_place(doc, *c, None));
        out.push(Check {
            kind: Kind::MixedPotentials,
            page: p,
            at: a,
            object: odd.map_or(object, |c| Object::Conductor(*c)),
            which: net.potentials.keys().cloned().collect::<Vec<_>>().join(", "),
            fix: None,
        });
    }
    // A continuation name that is neither in the potentials table nor the net's own number (a
    // pair placed by Merge the nets): a warning, never a refusal (owner, 2026-11-09). Only names
    // in the table give a net its potential (design 022 §6 rule 2).
    for key in &net.names {
        let numbered = net.numbers.keys().any(|n| name_key(n) == *key);
        if doc.settings.wiring.potential(key).is_some() || numbered {
            continue;
        }
        let cont = net.terminals.iter().find_map(|t| {
            let x = doc.terminals.get(t)?;
            let c = doc.components.get(&x.component)?;
            (ecad_model::connectivity::continuation_key(&c.properties).as_ref() == Some(key)).then_some((*t, x.component))
        });
        let (p, a, o, which) = match cont {
            Some((t, c)) => {
                let (p, a) = terminal_place(doc, t);
                let name = doc.components.get(&c).and_then(|x| x.properties.get(NAME)).cloned();
                (p, a, Object::Component(c), name.unwrap_or_else(|| key.clone()))
            }
            None => (page, at, object, key.clone()),
        };
        out.push(Check { kind: Kind::UnknownName, page: p, at: a, object: o, which, fix: None });
    }
    out
}

// 032: the Looks connected row's words and fix (design 032 §2.7): two open ends that meet are
// joined by merging them (`joinwires`), not by a junction.
fn looks_connected_words() -> &'static str {
    "Looks connected"
}

// 032: see `looks_connected_words`.
fn looks_connected_fix(at: Pt) -> Fix {
    Fix::Join(at)
}

/// The checks of conductor `c`: open end, loose wire, looks connected, drawn away.
pub fn conductor_checks(doc: &Document, c: ConductorId, grid: &Grid, tol: f64) -> Vec<Check> {
    let mut out = Vec::new();
    let Some(cond) = doc.conductors.get(&c) else { return out };
    let which = conductor_words(doc, c);
    let loose: Vec<End> = End::BOTH.into_iter().filter(|e| cond.end(*e).is_none()).collect();
    match loose.len() {
        2 => {
            let (page, at) = conductor_place(doc, c, None);
            out.push(Check { kind: Kind::LooseWire, page, at, object: Object::Conductor(c), which: which.clone(), fix: None });
        }
        1 => {
            let (page, at) = conductor_place(doc, c, loose.first().copied());
            out.push(Check { kind: Kind::OpenEnd, page, at, object: Object::Conductor(c), which: which.clone(), fix: None });
        }
        _ => {}
    }
    // Looks connected: a loose end on another wire's interior, or on another loose end (reported
    // by the lower conductor ID only, so the pair is one row).
    for (page, v) in routes_of(doc, c) {
        for end in &loose {
            let at = if *end == End::A { v.first() } else { v.last() };
            let Some(at) = at.copied() else { continue };
            if let Some((other, is_end)) = grid.under(page, at, c) {
                if is_end && other < c {
                    continue;
                }
                out.push(Check {
                    kind: Kind::LooksConnected,
                    page: Some(page),
                    at: Some(at),
                    object: Object::Conductor(c),
                    which: which.clone(),
                    fix: Some(looks_connected_fix(at)),
                });
            }
        }
        // Drawn away: an attached end whose route end is not at its point on this page.
        for (end, at) in [(End::A, v.first()), (End::B, v.last())] {
            let (Some(t), Some(at)) = (cond.end(end), at) else { continue };
            if let Some(tp) = ecad_model::derive::terminal_position(doc, page, t)
                && !tp.near(*at, tol.max(1e-6))
            {
                out.push(Check {
                    kind: Kind::DrawnAway,
                    page: Some(page),
                    at: Some(*at),
                    object: Object::Conductor(c),
                    which: which.clone(),
                    fix: None,
                });
            }
        }
    }
    // Brief 040: a colour code the app cannot draw is drawn in ink and listed as information.
    if let Some(code) = crate::look::conductor_colour(doc, c).filter(|code| crate::look::colour_code(code).is_none()) {
        let (page, at) = conductor_place(doc, c, None);
        let code = crate::look::shorten(code.trim(), crate::look::MAX_CODE_CHARS);
        out.push(Check { kind: Kind::UnknownColour, page, at, object: Object::Conductor(c), which: format!("{which} {code}"), fix: None });
    }
    out
}

/// The checks of `page` as a whole: brief 040's dash budget ([`crate::look::dash_budget`]).
pub fn page_checks(doc: &Document, page: PageId) -> Vec<Check> {
    let left = crate::look::dash_budget(doc, page).left_out;
    if left.is_empty() {
        return Vec::new();
    }
    let n = left.len();
    let which = if n == 1 { "1 wire".to_string() } else { format!("{n} wires") };
    vec![Check { kind: Kind::DashesLeftOut, page: Some(page), at: None, object: Object::None, which, fix: None }]
}

/// Whether connection point `t` is one its symbol marks quiet (brief 034, design 034 §11): a
/// spare connector pin, fuse output or card supply point may stay unwired without an
/// `Unconnected` check. Every other check on it still runs.
fn is_quiet_point(doc: &Document, t: TerminalId) -> bool {
    let Some(c) = doc.terminals.get(&t).map(|x| x.component) else { return false };
    doc.representations_of(Subject::Component(c)).any(|(_, r)| {
        let Some(s) = crate::devices::rep_starter(doc, r) else { return false };
        let Some((def, _)) = crate::devices::rep_definition(doc, r) else { return false };
        crate::devices::shown_terminals(doc, c, r)
            .iter()
            .filter(|(x, _)| *x == t)
            .filter_map(|(_, at)| def.terminals.iter().find(|td| td.at.near(*at, 1e-9)))
            .any(|td| crate::symbols::is_quiet(s, &td.key))
    })
}

/// The checks of connection point `t`: a device point with nothing attached.
pub fn terminal_checks(doc: &Document, t: TerminalId) -> Vec<Check> {
    let Some(term) = doc.terminals.get(&t) else { return Vec::new() };
    if matches!(kind_of(doc, term.component), Some(JUNCTION | CONTINUATION)) || doc.has_ends(t) {
        return Vec::new();
    }
    // Bridged inside its device to an attached point: connected.
    let conn = doc.connectivity();
    if conn.net_of_terminal(t).and_then(|id| conn.net(id)).is_some_and(|n| !n.conductors.is_empty()) {
        return Vec::new();
    }
    if is_quiet_point(doc, t) {
        return Vec::new();
    }
    let (page, at) = terminal_place(doc, t);
    vec![Check { kind: Kind::Unconnected, page, at, object: Object::Terminal(t), which: wires::point_words(doc, t), fix: None }]
}

/// The checks of component `c`: a junction with fewer than three wires, a continuation with no
/// partner.
pub fn component_checks(doc: &Document, c: ComponentId) -> Vec<Check> {
    let mut out = Vec::new();
    let place = || doc.terminals_of(c).next().map_or((None, None), |(t, _)| terminal_place(doc, t));
    match kind_of(doc, c) {
        Some(JUNCTION) => {
            let wires_here: usize = doc.terminals_of(c).map(|(t, _)| doc.ends_at(t).len()).sum();
            if wires_here < 3 {
                let (page, at) = place();
                out.push(Check { kind: Kind::ThinJunction, page, at, object: Object::Component(c), which: format!("{wires_here} wires"), fix: None });
            }
        }
        Some(CONTINUATION) => {
            let name = doc.components.get(&c).and_then(|x| x.properties.get(NAME)).cloned().unwrap_or_default();
            let partners = doc.continuations_named(&name_key(&name)).len();
            if partners < 2 {
                let (page, at) = place();
                out.push(Check { kind: Kind::LonelyContinuation, page, at, object: Object::Component(c), which: name, fix: None });
            }
        }
        _ => {
            // Brief 026: a provisional tag, and a tag a cable holds too.
            if let Some(t) = crate::devices::tag_of(doc, c)
                && (ecad_model::tag::is_provisional(t) || !doc.cables_tagged(t).is_empty())
            {
                // Located only when there is a row to show (a build visits every device).
                let (page, at) = ecad_model::numbering::rep_anchor(doc, Subject::Component(c)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
                if ecad_model::tag::is_provisional(t) {
                    out.push(Check {
                        kind: Kind::ProvisionalTag,
                        page,
                        at,
                        object: Object::Component(c),
                        which: t.to_string(),
                        fix: Some(Fix::Finalise),
                    });
                }
                if !doc.cables_tagged(t).is_empty() {
                    out.push(Check {
                        kind: Kind::DuplicateTag,
                        page,
                        at,
                        object: Object::Component(c),
                        which: format!("A cable and a device share {t}"),
                        fix: None,
                    });
                }
            }
        }
    }
    // Brief 025 (design 025 §4.3): drawn beyond the part's offer, ringed at the first such pole.
    for (which, page, at) in crate::xref::count_checks(doc, c) {
        out.push(Check { kind: Kind::TooManyContacts, page, at, object: Object::Component(c), which, fix: None });
    }
    // Brief 044 (design 044 §6.3): a connector's mating checks.
    out.extend(crate::connectors::checks(doc, c));
    // Brief 033 (design 033 §4.1): a table the user placed over the title block stays there.
    if let Some((which, page, at)) = crate::xref::title_check(doc, c) {
        out.push(Check {
            kind: Kind::TableOverTitle,
            page: Some(page),
            at: Some(at),
            object: Object::Component(c),
            which,
            fix: Some(Fix::MoveTable(c)),
        });
    }
    out
}

/// The checks of cable `k`'s reference ends (brief 027, design 027 §4.2): a cable with exactly one
/// reference end has an orphan (`/?`). Reads that cable's representations only.
pub fn reference_checks(doc: &Document, k: CableId) -> Vec<Check> {
    let refs = crate::references::references_of(doc, k);
    let [(rid, r)] = refs.as_slice() else { return Vec::new() };
    let at = r.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
        ecad_model::ItemKind::Primitive(ecad_model::Primitive::Point { at }) if at.is_finite() => Some(*at),
        _ => None,
    });
    let which = format!("{} /?", crate::views::cable_title(doc, k));
    vec![Check { kind: Kind::OrphanReference, page: Some(r.page), at, object: Object::Reference(*rid), which, fix: None }]
}

/// The checks of cable `k` (brief 026): a provisional tag; a tag another cable or a device holds.
pub fn cable_checks(doc: &Document, k: ecad_model::CableId) -> Vec<Check> {
    let mut out = Vec::new();
    let Some(t) = crate::devices::cable_tag_of(doc, k) else { return out };
    if !ecad_model::tag::is_provisional(t) && doc.cables_tagged(t).len() < 2 && doc.components_tagged(t).is_empty() {
        return out;
    }
    let (page, at) = ecad_model::numbering::rep_anchor(doc, Subject::Cable(k)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
    if ecad_model::tag::is_provisional(t) {
        out.push(Check { kind: Kind::ProvisionalTag, page, at, object: Object::Cable(k), which: t.to_string(), fix: Some(Fix::Finalise) });
    }
    if doc.cables_tagged(t).len() > 1 {
        out.push(Check { kind: Kind::DuplicateTag, page, at, object: Object::Cable(k), which: format!("Two cables share {t}"), fix: None });
    } else if !doc.components_tagged(t).is_empty() {
        out.push(Check { kind: Kind::DuplicateTag, page, at, object: Object::Cable(k), which: format!("A cable and a device share {t}"), fix: None });
    }
    out
}

/// The kept Checks list (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct Checks {
    by_key: BTreeMap<Key, Vec<Check>>,
    grid: Grid,
    /// Nets with conductors and no number (for the one Unnumbered row).
    unnumbered: BTreeSet<NetId>,
    /// Provisional tags are on (brief 026): the Unnumbered row is a warning, `… provisional`.
    provisional: bool,
    /// Bumped on every change, for views that copy the rows.
    pub revision: u64,
    /// Rows by severity (errors, warnings, info), kept by [`Checks::set`] (brief 027's badge reads
    /// them; nothing recounts the rows).
    counts: [usize; 3],
    /// Bumped only when some row actually changed (an update that re-checked subjects and found
    /// the same rows leaves it), for views that show the rows themselves (the Release dialog).
    pub changed: u64,
}

impl Checks {
    /// The whole list, computed from scratch (open, reload, first show).
    pub fn build(doc: &Document, tol: f64) -> Self {
        let mut ch = Checks::default();
        for c in doc.conductors.keys() {
            ch.grid.insert(doc, *c);
        }
        let conn = doc.connectivity();
        for (id, net) in conn.nets() {
            ch.set(Key::Net(id), net_checks(doc, id, net));
            if !net.conductors.is_empty() && net.numbers.is_empty() {
                ch.unnumbered.insert(id);
            }
        }
        for c in doc.conductors.keys() {
            let v = conductor_checks(doc, *c, &ch.grid, tol);
            ch.set(Key::Conductor(*c), v);
        }
        for t in doc.terminals.keys() {
            ch.set(Key::Terminal(*t), terminal_checks(doc, *t));
        }
        for c in doc.components.keys() {
            ch.set(Key::Component(*c), component_checks(doc, *c));
        }
        for k in doc.cables.keys() {
            ch.set(Key::CableReferences(*k), reference_checks(doc, *k));
            ch.set(Key::Cable(*k), cable_checks(doc, *k));
        }
        for page in doc.pages.keys() {
            ch.set(Key::Page(*page), page_checks(doc, *page));
        }
        ch.set(Key::Styles, crate::styles::checks::rows(doc));
        ch.provisional = ecad_model::numbering::provisional_on(doc);
        ch.set_unnumbered();
        ch
    }

    fn set(&mut self, key: Key, v: Vec<Check>) {
        let same = match self.by_key.get(&key) {
            Some(old) => *old == v,
            None => v.is_empty(),
        };
        if same {
            return;
        }
        for c in &v {
            if let Some(n) = self.counts.get_mut(c.severity() as usize) {
                *n += 1;
            }
        }
        let old = if v.is_empty() { self.by_key.remove(&key) } else { self.by_key.insert(key, v) };
        for c in old.iter().flatten() {
            if let Some(n) = self.counts.get_mut(c.severity() as usize) {
                *n = n.saturating_sub(1);
            }
        }
        self.changed += 1;
    }

    /// Rows by severity: (errors, warnings, info). O(1): kept by every change.
    pub fn counts(&self) -> (usize, usize, usize) {
        (self.counts[0], self.counts[1], self.counts[2])
    }

    fn set_unnumbered(&mut self) {
        let n = self.unnumbered.len();
        let row = (n > 0).then(|| {
            if self.provisional {
                Check {
                    kind: Kind::ProvisionalWires,
                    page: None,
                    at: None,
                    object: Object::None,
                    which: if n == 1 { "1 wire number provisional".into() } else { format!("{n} wire numbers provisional") },
                    fix: Some(Fix::Finalise),
                }
            } else {
                Check {
                    kind: Kind::Unnumbered,
                    page: None,
                    at: None,
                    object: Object::None,
                    which: if n == 1 { "1 net has no number".into() } else { format!("{n} nets have no number") },
                    fix: Some(Fix::NumberThem),
                }
            }
        });
        self.set(Key::Project, row.into_iter().collect());
    }

    /// Every row, in no particular order.
    pub fn rows(&self) -> impl Iterator<Item = &Check> {
        self.by_key.values().flatten()
    }

    /// How many rows.
    pub fn len(&self) -> usize {
        self.by_key.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    /// The rows sorted in book order (design 022 §8).
    pub fn sorted(&self, doc: &Document) -> Vec<Check> {
        sort(doc, self.rows().cloned().collect())
    }

    /// What `ops` (about to be committed on `before`) could change, worked out on the document
    /// before them: the subjects to re-check after them. Pair with [`Checks::update`].
    pub fn scope(&self, before: &Document, ops: &[Op]) -> Scope {
        let mut s = Scope::default();
        s.collect(before, ops, &self.grid);
        s
    }

    /// Re-check the subjects in `scope` (from [`Checks::scope`]) on `doc`, the state after the ops,
    /// adding what the ops themselves name in this state.
    pub fn update(&mut self, doc: &Document, ops: &[Op], mut scope: Scope, tol: f64) {
        scope.collect(doc, ops, &self.grid);
        if scope.all {
            *self = Checks { revision: self.revision + 1, changed: self.changed + 1, ..Checks::build(doc, tol) };
            return;
        }
        // Geometry first: the grid must hold the new routes before any conductor is re-checked.
        for c in &scope.routes {
            let old = self.grid.cells(*c);
            self.grid.insert(doc, *c);
            let new = self.grid.cells(*c);
            let near: BTreeSet<Cell> = old.union(&new).copied().collect();
            scope.conductors.extend(self.grid.loose_near(&near));
        }
        let conn = doc.connectivity();
        // Nets: those named, and those sharing their numbers.
        let mut nets: BTreeSet<NetId> = BTreeSet::new();
        let mut numbers = std::mem::take(&mut scope.numbers);
        for n in &scope.nodes {
            if let Some(id) = conn.net_of(n) {
                nets.insert(id);
            }
        }
        for id in &nets {
            if let Some(net) = conn.net(*id) {
                numbers.extend(net.numbers.keys().cloned());
            }
        }
        // An unnumbered potential's shared number (`PE`) is never a shared-number error, so the
        // other nets carrying it have nothing to re-check (otherwise every PE edit would re-check
        // every PE net in the project).
        let exempt = |n: &str| doc.settings.wiring.potentials.iter().any(|p| !p.numbered && p.prefix == n);
        for n in numbers.iter().filter(|n| !exempt(n)) {
            nets.extend(conn.nets_numbered(n).map(|(id, _)| id));
        }
        // Nets that no longer exist lose their rows; those that do are checked again.
        for id in scope.old_nets.iter().chain(nets.iter()) {
            match conn.net(*id) {
                Some(net) => {
                    self.set(Key::Net(*id), net_checks(doc, *id, net));
                    if !net.conductors.is_empty() && net.numbers.is_empty() {
                        self.unnumbered.insert(*id);
                    } else {
                        self.unnumbered.remove(id);
                    }
                }
                None => {
                    self.set(Key::Net(*id), Vec::new());
                    self.unnumbered.remove(id);
                }
            }
        }
        for c in &scope.conductors {
            let v = conductor_checks(doc, *c, &self.grid, tol);
            self.set(Key::Conductor(*c), v);
        }
        for t in &scope.terminals {
            self.set(Key::Terminal(*t), terminal_checks(doc, *t));
        }
        for c in &scope.components {
            self.set(Key::Component(*c), component_checks(doc, *c));
        }
        for k in &scope.cable_references {
            self.set(Key::CableReferences(*k), reference_checks(doc, *k));
        }
        for k in &scope.cables {
            self.set(Key::Cable(*k), cable_checks(doc, *k));
        }
        // Brief 040: the dash budget of the pages the re-checked wires are on, and of the pages
        // that had a row (a wire removed or recoloured there).
        let mut pages: BTreeSet<PageId> = self.by_key.keys().filter_map(|k| if let Key::Page(p) = k { Some(*p) } else { None }).collect();
        for c in scope.conductors.iter().chain(scope.routes.iter()) {
            pages.extend(doc.representations_of(Subject::Conductor(*c)).map(|(_, r)| r.page));
        }
        for page in pages {
            let rows = if doc.pages.contains_key(&page) { page_checks(doc, page) } else { Vec::new() };
            self.set(Key::Page(page), rows);
        }
        if scope.styles {
            self.set(Key::Styles, crate::styles::checks::rows(doc));
        }
        self.provisional = ecad_model::numbering::provisional_on(doc);
        self.set_unnumbered();
        self.revision += 1;
    }
}

/// The subjects a transaction could change (see the module's update map).
#[derive(Debug, Clone, Default)]
pub struct Scope {
    all: bool,
    /// Only where rows are (page numbers, folders) changed: the revision moves, so copies of the
    /// rows are taken again.
    relocate: bool,
    nodes: BTreeSet<ecad_model::Node>,
    old_nets: BTreeSet<NetId>,
    numbers: BTreeSet<String>,
    conductors: BTreeSet<ConductorId>,
    routes: BTreeSet<ConductorId>,
    terminals: BTreeSet<TerminalId>,
    components: BTreeSet<ComponentId>,
    /// Cables whose reference ends to re-check (brief 027: orphans).
    cable_references: BTreeSet<CableId>,
    /// Cables to re-check (brief 026: provisional and shared tags).
    cables: BTreeSet<ecad_model::CableId>,
    /// Brief 050a: the style rows (`crate::styles::checks::touches`).
    styles: bool,
}

impl Scope {
    /// Add what `ops` name, looked up in `doc` (called on the state before and after).
    fn collect(&mut self, doc: &Document, ops: &[Op], _grid: &Grid) {
        use ecad_model::Node;
        let conn = doc.connectivity();
        let node = |s: &mut Scope, n: Node| {
            if let Some(id) = conn.net_of(&n) {
                s.old_nets.insert(id);
                if let Some(net) = conn.net(id) {
                    s.numbers.extend(net.numbers.keys().cloned());
                }
            }
            s.nodes.insert(n);
        };
        for op in ops {
            self.styles |= crate::styles::checks::touches(op);
            match op {
                Op::CreateConductor { id, conductor } | Op::DeleteConductor { id, conductor } => {
                    self.conductors.insert(*id);
                    self.routes.insert(*id);
                    node(self, Node::Conductor(*id));
                    for t in conductor.ends.iter().flatten() {
                        self.terminals.insert(*t);
                        node(self, Node::Terminal(*t));
                        self.component_of(doc, *t);
                    }
                }
                Op::AttachEnd { conductor, before, after, .. } => {
                    self.conductors.insert(*conductor);
                    self.routes.insert(*conductor);
                    node(self, Node::Conductor(*conductor));
                    for t in [before, after].into_iter().flatten() {
                        self.terminals.insert(*t);
                        node(self, Node::Terminal(*t));
                        self.component_of(doc, *t);
                    }
                }
                Op::AddTerminal { id, terminal } | Op::RemoveTerminal { id, terminal } => {
                    self.terminals.insert(*id);
                    self.components.insert(terminal.component);
                    node(self, Node::Terminal(*id));
                }
                // Brief 048: another definition may bridge other points.
                Op::SetComponentDefinition { id, .. } => {
                    self.components.insert(*id);
                    for (t, _) in doc.terminals_of(*id) {
                        node(self, Node::Terminal(t));
                    }
                }
                Op::CreateComponent { id, component } | Op::DeleteComponent { id, component } => {
                    self.tag_holders(doc, component.properties.get(ecad_model::tag::TAG_KEY));
                    self.component(doc, *id, component.properties.get(NAME).map(String::as_str));
                    for (t, _) in doc.terminals_of(*id) {
                        node(self, Node::Terminal(t));
                    }
                }
                Op::SetProperty { object: ObjectRef::Component(c), key, before, after } => {
                    if key == ecad_model::tag::TAG_KEY {
                        self.tag_holders(doc, [before, after].into_iter().flatten());
                    }
                    for n in [before, after].into_iter().flatten() {
                        self.component(doc, *c, Some(n));
                        node(self, Node::Name(name_key(n)));
                    }
                    self.component(doc, *c, None);
                    for (t, _) in doc.terminals_of(*c) {
                        node(self, Node::Terminal(t));
                    }
                }
                Op::SetProperty { object: ObjectRef::Conductor(c), before, after, .. } => {
                    node(self, Node::Conductor(*c));
                    self.conductors.insert(*c);
                    self.numbers.extend(before.iter().chain(after.iter()).cloned());
                }
                Op::SetProperty { object: ObjectRef::Cable(k), .. }
                | Op::CreateCable { id: k, .. }
                | Op::DeleteCable { id: k, .. }
                | Op::AddCableMember { cable: k, .. }
                | Op::RemoveCableMember { cable: k, .. } => {
                    // Brief 027: a new or deleted cable, or its tag (the orphan row's words).
                    self.cable_references.insert(*k);
                    // Brief 026: the cable's own rows, and every holder of the tags it had or has.
                    self.cables.insert(*k);
                    match op {
                        Op::SetProperty { key, before, after, .. } if key == ecad_model::tag::TAG_KEY => {
                            self.tag_holders(doc, [before, after].into_iter().flatten());
                        }
                        Op::CreateCable { cable, .. } | Op::DeleteCable { cable, .. } => {
                            self.tag_holders(doc, cable.properties.get(ecad_model::tag::TAG_KEY));
                        }
                        _ => {}
                    }
                    // A cable's tag and cores are part of its members' words.
                    if let Some(cable) = doc.cables.get(k) {
                        self.conductors.extend(cable.members.iter().map(|m| m.conductor));
                    }
                    if let Op::AddCableMember { member, .. } | Op::RemoveCableMember { member, .. } = op {
                        self.conductors.insert(member.conductor);
                    }
                }
                Op::AddItem { id, .. } | Op::RemoveItem { id, .. } => self.item(doc, *id),
                Op::ModifyItem { id, .. } => self.item(doc, *id),
                Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => self.rep(doc, rep),
                Op::ModifyRepresentation { before, after, .. } => {
                    self.rep(doc, before);
                    self.rep(doc, after);
                }
                Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => {
                    for c in doc.components_using(*id) {
                        self.components.insert(c);
                        for (t, _) in doc.terminals_of(c) {
                            self.terminals.insert(t);
                            node(self, Node::Terminal(t));
                        }
                    }
                }
                // Only the numbering and potential settings change what checks find; sheet, page
                // numbering and reference style change where rows say they are; label looks
                // (colour style, height), tag settings and units change nothing here.
                Op::SetProjectSettings { before, after } => {
                    // Brief 033 (review of #64, 1): a table the user placed may enter or leave the
                    // title block when the project's sheet changes.
                    if before.sheet != after.sheet {
                        self.components.extend(crate::xref::placed_tables(doc));
                    }
                    let (b, a) = (&before.wiring, &after.wiring);
                    if b.scheme != a.scheme || b.digits != a.digits || b.unclassified_prefix != a.unclassified_prefix || b.potentials != a.potentials
                    {
                        self.all = true;
                    } else if before.sheet != after.sheet
                        || before.page_number_step != after.page_number_step
                        || before.reference_style != after.reference_style
                    {
                        self.relocate = true;
                    }
                }
                // Brief 033 (review of #64, 1): a page's sheet moves its frame, so the devices on it
                // are checked again for a table over the title block.
                Op::SetProperty { object: ObjectRef::Page(p), key, .. } if key == ecad_model::sheet::SHEET_KEY => {
                    self.relocate = true;
                    for (_, r) in doc.representations_on(*p) {
                        if let Subject::Component(c) = r.subject {
                            self.components.insert(c);
                        }
                    }
                }
                // Page numbers, folders and the page order change where rows say they are (brief
                // 019), not what they find: the rows are copied again.
                Op::SetProperty { object: ObjectRef::Page(_) | ObjectRef::Project | ObjectRef::Standard, .. }
                | Op::AddFolder { .. }
                | Op::RemoveFolder { .. }
                | Op::ModifyFolder { .. }
                | Op::AddPage { .. } => self.relocate = true,
                // Brief 027: a removed page's reference ends go with it.
                Op::RemovePage { id, .. } => {
                    self.relocate = true;
                    for (_, r) in doc.representations_on(*id) {
                        if let Subject::Cable(k) = r.subject {
                            self.cable_references.insert(k);
                        }
                    }
                }
                Op::AddLayer { .. } | Op::RemoveLayer { .. } | Op::NameVersion { .. } | Op::UnnameVersion { .. } => {}
                // Brief 050a: style records and the standard reach only the style rows (above).
                Op::SetStyle { .. } | Op::SetStandard { .. } => {}
            }
        }
    }

    /// Every device and cable holding one of `tags` (a shared tag's rows on both sides).
    fn tag_holders<'t>(&mut self, doc: &Document, tags: impl IntoIterator<Item = &'t String>) {
        for t in tags {
            self.components.extend(doc.components_tagged(t));
            self.cables.extend(doc.cables_tagged(t));
        }
    }

    fn component_of(&mut self, doc: &Document, t: TerminalId) {
        if let Some(c) = doc.terminals.get(&t).map(|x| x.component) {
            self.components.insert(c);
        }
    }

    /// A component and, for a continuation name, every continuation with that name.
    fn component(&mut self, doc: &Document, c: ComponentId, name: Option<&str>) {
        self.components.insert(c);
        let name = name.map(str::to_string).or_else(|| doc.components.get(&c).and_then(|x| x.properties.get(NAME).cloned()));
        if let Some(n) = name {
            self.components.extend(doc.continuations_named(&name_key(&n)));
        }
        for (t, _) in doc.terminals_of(c) {
            self.terminals.insert(t);
        }
    }

    /// An item's representation (a new item's representation op names it instead).
    fn item(&mut self, doc: &Document, id: ecad_model::ItemId) {
        if let Some((_, r)) = doc.representation_of_item(id) {
            let r = r.clone();
            self.rep(doc, &r);
        }
    }

    fn rep(&mut self, doc: &Document, r: &ecad_model::Representation) {
        match r.subject {
            Subject::Conductor(c) => {
                self.conductors.insert(c);
                self.routes.insert(c);
            }
            Subject::Component(c) => {
                // Its points moved: the wires attached to them may now be drawn away; a
                // continuation's place moves its partners' rows.
                self.component(doc, c, None);
                for (t, _) in doc.terminals_of(c) {
                    self.conductors.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                }
            }
            // A cable's drawing moves its rows (brief 026); a reference end linked, changed or
            // unlinked (its point an item op) re-derives its orphans (brief 027).
            Subject::Cable(k) => {
                self.cables.insert(k);
                self.cable_references.insert(k);
            }
        }
    }
}

/// The drawn handles of `object` on `page` (for a go-to's selection): a conductor's route, a
/// component's symbols, a connection point's symbol.
pub fn handles_of(p: &crate::Project, page: PageId, object: Object) -> Vec<cadcraft_engine::doc::Handle> {
    let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return Vec::new() };
    let doc = p.ledger.document();
    let subject = match object {
        Object::Conductor(c) => Subject::Conductor(c),
        Object::Component(c) => Subject::Component(c),
        Object::Reference(r) => {
            return doc
                .representations
                .get(&r)
                .filter(|x| x.page == page)
                .map(|x| x.items.iter().filter_map(|i| map.item(*i)).collect())
                .unwrap_or_default();
        }
        Object::Cable(k) => Subject::Cable(k),
        Object::Terminal(t) => match doc.terminals.get(&t) {
            Some(x) => Subject::Component(x.component),
            None => return Vec::new(),
        },
        Object::None => return Vec::new(),
    };
    doc.representations_of(subject).filter(|(_, r)| r.page == page).flat_map(|(_, r)| r.items.iter().filter_map(|i| map.item(*i))).collect()
}

/// `rows` in book order.
pub fn sort(doc: &Document, mut rows: Vec<Check>) -> Vec<Check> {
    let order: HashMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    rows.sort_by_key(|c| c.order_key(&order));
    rows
}

/// Every check of the project, worked out from scratch (the `checks` command reads on request).
pub fn all(doc: &Document, tol: f64) -> Vec<Check> {
    all_with(doc, tol, &crate::outputs::Outputs::build(doc))
}

/// [`all`] with the report models already kept (brief 024: the project's, reused by `checks`).
pub fn all_with(doc: &Document, tol: f64, outputs: &crate::outputs::Outputs) -> Vec<Check> {
    let mut rows: Vec<Check> = Checks::build(doc, tol).rows().cloned().collect();
    rows.extend(outputs.checks());
    sort(doc, rows)
}
