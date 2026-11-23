//! The Checks list: open ends, loose wires, false-looking connections,
//! nets with one connection point, unconnected device points, number and potential conflicts.
//!
//! # Semantics
//! - Every check is derived from the document; nothing is stored.
//! - Checks belong to a **subject**: a net, a conductor, a connection point, a component, or the
//!   project (the unnumbered-nets row). `Checks` keeps the checks by subject and is built whole
//!   only on open, reload or first show (`Checks::build`); after that `Checks::update` re-checks
//!   only the subjects a transaction's ops could change (the update map below). The geometric
//!   checks use a per-page grid of route segments and loose ends ([`GRID_MM`] cells), built with
//!   the list and kept by the same updates.
//!
//! # Update map
//! | Op | Re-checked |
//! | --- | --- |
//! | `CreateWire` / `DeleteWire` (the commit's wire rewrite included) | that conductor; the connection points at its ends; their nets (before and after) and the nets sharing their numbers |
//! | `AddRoute` / `RemoveRoute` / `ModifyRoute` | that route (open end, loose route, looks connected, drawn away); free route ends near its old and new path; the connection points it lands on before and after |
//! | `AddConnectionPoint` / `RemoveConnectionPoint` | that point and its net |
//! | `CreateDevice` / `DeleteDevice` / `SetProperty` component | that component (its contact count against its part's `contacts`); its points' nets; continuations with its name (partners) |
//! | `SetProperty` conductor | that conductor's net, and the nets sharing its old and new number |
//! | `AddItem` / `ModifyItem` / `RemoveItem` / representation ops of a route | that route |
//! | the same of a component symbol | routes landing on its points ("drawn away"); continuations with its name |
//! | `EmbedDefinition` / `RemoveDefinition` | components using it and their points |
//! | `SetProjectSettings` changing the numbering or potentials (`wiring` scheme, digits, prefix, table) | every net (rare and explicit) |
//! | `SetProjectSettings` changing anything else (label looks, tags, sheet, reference style) | nothing (sheet and reference style: rows are copied again) |
//! | representation ops of a cable (reference ends), `CreateCable` / `DeleteCable` / cable ops | that cable's reference ends (orphan) |
//! | `RemovePage` | the cables with a reference end on it (orphan) |
//! | pages, layers, versions | nothing (page texts are worked out when shown) |
//!
//! This module reads the document; it changes nothing.

use ecad_model::naming::numbering::Places;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use ecad_model::connectivity::name_key;
use ecad_model::connectivity::wiring;
use ecad_model::document::effect::Effect;
use ecad_model::{CableId, ConnectionPointId, DeviceId, Document, End, NetData, NetKey, ObjectRef, PageId, Pt, RouteEnd, RouteId, Subject, WireId};
use serde_json::{Value, json};

use crate::wires::{self, ON_WIRE};
use ecad_model::look::Look;

pub mod cables;
pub mod devices;
mod grid;
pub mod kept;
pub mod nets;
pub mod places;
pub(crate) mod view;
pub mod words_check;

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

/// One row per check kind: its variant, its JSON name
/// (persisted: never renamed), its severity, the words of the Show list and, after them, `count`
/// for a row that shows a count rather than a place (`12 wires`). Generates [`Kind`],
/// [`Kind::ALL`] (in table order), [`Kind::name`], [`Kind::severity`], [`Kind::words`] and
/// [`Kind::is_count`]; a new check is one row here plus its source module.
macro_rules! check_kinds {
    ($($(#[$doc:meta])* $kind:ident = $name:literal, $severity:ident, $words:expr $(, $count:ident)?;)*) => {
        /// What a check found (in the table's order).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Kind {
            $($(#[$doc])* $kind,)*
        }

        impl Kind {
            /// Every kind, in table order (the Show list's order).
            pub const ALL: [Kind; [$(stringify!($kind)),*].len()] = [$(Kind::$kind),*];

            /// The JSON name.
            pub fn name(self) -> &'static str {
                match self {
                    $(Kind::$kind => $name,)*
                }
            }

            /// The words of the Show list.
            pub fn words(self) -> &'static str {
                match self {
                    $(Kind::$kind => $words,)*
                }
            }

            pub fn severity(self) -> Severity {
                match self {
                    $(Kind::$kind => Severity::$severity,)*
                }
            }

            /// Whether the kind's row shows a count rather than a place (`12 wires`).
            pub fn is_count(self) -> bool {
                match self {
                    $(Kind::$kind => check_kinds!(@count $($count)?),)*
                }
            }
        }
    };
    (@count count) => {
        true
    };
    (@count) => {
        false
    };
}

#[rustfmt::skip]
check_kinds! {
    JoinsPotentials = "joins-potentials", Error, "Net joins two potentials";
    SharedNumber = "shared-number", Error, "Two nets share a number";
    TwoFixed = "two-fixed", Error, "One net, two fixed numbers";
    OpenEnd = "open-end", Warning, "Open end";
    LooseWire = "loose-wire", Warning, "Loose wire";
    /// A drawn line with a free end carries no wire until both ends land.
    NoWireYet = "no-wire-yet", Info, "Line not connected at both ends: no wire yet";
    LooksConnected = "looks-connected", Warning, nets::looks_connected_words();
    DrawnAway = "drawn-away", Warning, "End drawn away from its connection point";
    SinglePoint = "single-point", Warning, "Net with a single connection point";
    Unconnected = "unconnected", Info, "Connection point not connected";
    /// A connection point with more wires than it accepts.
    OverCount = "over-count", Error, "Connection point has more wires than it accepts";
    LonelyContinuation = "lonely-continuation", Info, "Continuation without partner";
    MixedPotentials = "mixed-potentials", Info, "Mixed potentials in one net";
    Unnumbered = "unnumbered", Info, "Unnumbered nets", count;
    UnknownName = "unknown-name", Warning, "Continuation name not a potential";
    /// Two cables, or a cable and a device, share a tag (one namespace).
    DuplicateTag = "duplicate-tag", Error, "Two cables share a tag";
    /// A provisional tag.
    ProvisionalTag = "provisional-tag", Warning, "Provisional tag";
    /// Nets without a number while provisional tags are on: one summary row.
    ProvisionalWires = "provisional-wires", Warning, "Wire numbers provisional", count;
    /// A device's stored location is the context of none of its
    /// symbols.
    LocationUnused = "location-unused", Warning, "Location used by none of its symbols";
    /// The same for its function.
    FunctionUnused = "function-unused", Warning, "Function used by none of its symbols";
    /// A device whose location was set by hand has a symbol inside a
    /// location box of another location; one row on every placed symbol of the device.
    TiedLocations = "symbol-tied-locations", Error, "Symbol tied to multiple locations";
    /// The same for its function.
    TiedFunctions = "symbol-tied-functions", Error, "Symbol tied to multiple functions";
    /// Devices without a function (one count row, when some function is set).
    NoFunction = "function-none", Info, "Devices without a function";
    /// A stored function or location names no entry of the project's list.
    DesignationMissing = "designation-missing", Warning, "Function or location no longer in the list";
    /// Two entries of one aspect share a code (only a branch merge makes them).
    DesignationDuplicate = "designation-duplicate", Warning, "Two entries share a code";
    /// A device with two heads (only a merge makes them).
    TwoHeads = "two-heads", Warning, "A device with two heads";
    /// A location box outline crosses itself.
    BoxCrosses = "box-crosses", Warning, "Location box outline crosses itself";
    /// Two location boxes overlap without nesting.
    BoxesOverlap = "boxes-overlap", Info, "Location boxes overlap without nesting";
    /// Devices without a part number (one count row).
    NoPartNumber = "no-part-number", Info, "Devices without a part number", count;
    /// Devices without a location (one count row).
    NoLocation = "no-location", Info, "Devices without a location", count;
    /// A device with more contacts of a kind drawn than its part offers.
    TooManyContacts = "too-many-contacts", Warning, "More contacts than the part offers";
    /// A device whose group has typed rating fields, none
    /// set (a rating note does not count).
    NoRating = "no-rating", Warning, "Device with no rating";
    /// A strip with more terminals drawn than it offers.
    TooManyTerminalBlocks = "too-many-terminal-blocks", Warning, "More terminals than the strip offers";
    /// Two terminals of one strip with one label.
    TerminalBlockTwice = "terminal-block-twice", Error, "Terminal drawn twice";
    /// A cable with more members than cores offered.
    TooManyCores = "too-many-cores", Warning, "More cores used than the cable offers";
    /// Two members of one cable with one core label.
    CoreTwice = "core-twice", Warning, "Core used by two wires";
    /// Cables with no core count (one count row).
    NoCoreCount = "no-core-count", Info, "Cables without a core count", count;
    /// A cable reference end with no other end.
    OrphanReference = "orphan-reference", Warning, "Cable reference with no other end";
    /// An accessory placed after or on a position the strip lacks.
    AccessoryOrphan = "accessory-orphan", Warning, "Accessory at a position the strip lacks";
    /// Placed strip accessories without a part number (one count row).
    NoAccessoryPart = "no-accessory-part", Info, "Strip accessories without a part number", count;
    /// A link spanning a partition.
    JumperBarAcrossPartition = "jumper-bar-across-partition", Warning, "Link crosses a partition";
    /// A link naming positions the strip does not have.
    JumperBarOrphan = "jumper-bar-orphan", Warning, "Link names positions the strip lacks";
    /// A `jumper_bar.<n>` key on a device that is not a terminal strip.
    JumperBarOnNonStrip = "jumper-bar-on-non-strip", Warning, "Link on a device that is not a strip";
    /// A contact table the user placed overlaps the title block.
    TableOverTitle = "table-over-title", Warning, "Contact table over the title block";
    /// A generated page grew onto another page's number.
    ContinuationClash = "continuation-clash", Warning, "Continuation sheet has another page's number";
    /// A level of a multi-level position drawn twice.
    LevelTwice = "level-twice", Error, "Terminal level drawn twice";
    /// A position whose levels are drawn by symbols of two types.
    PositionMixed = "position-mixed", Warning, "Position mixes terminal types";
    /// A link naming a level its position does not have.
    LevelMissing = "level-missing", Warning, "Link names a level the position lacks";
    /// Fuse terminals that are devices of their own (one count row).
    FuseDevice = "fuse-device", Info, "Fuse terminals that are devices of their own";
    /// Fuse positions with no rating (one count row).
    NoFuseRating = "no-fuse-rating", Info, "Fuse positions without a rating";
    /// A wire or core colour code the app does not know (drawn in ink).
    UnknownColour = "unknown-colour", Info, "Colour code not defined (drawn in ink); define it in Standards";
    /// A page with more two-colour dashes than it draws (one row per page).
    DashesLeftOut = "dashes-left-out", Warning, "Two-colour wires drawn in one colour (too many dashes on the page)";
    /// Two cards in one slot of a rack.
    SlotTwice = "slot-twice", Error, "Two cards in one slot";
    /// A card naming a rack that is not in the project.
    RackMissing = "rack-missing", Warning, "Card names a rack that is not in the project";
    /// A card in a slot its rack does not have (or in a rack but no slot).
    SlotRange = "slot-range", Warning, "Card in a slot its rack does not have";
    /// PLC cards in no rack (one count row).
    Unracked = "unracked", Info, "PLC cards in no rack", count;
    /// One channel drawn twice.
    ChannelTwice = "channel-twice", Error, "Channel drawn twice";
    /// A channel drawn beyond its card's channel count.
    ChannelOver = "channel-over", Warning, "Channel beyond the card's count";
    /// A channel drawn by a symbol of another card type.
    ChannelType = "channel-type", Warning, "Channel drawn as another card type";
    /// Two cards' address blocks overlap.
    AddressOverlap = "address-overlap", Warning, "Address blocks overlap";
    /// One address given to two channels (one typed).
    AddressTwice = "address-twice", Warning, "Address given twice";
    /// A block past byte 65 535.
    AddressOverflow = "address-overflow", Warning, "Address block past byte 65535";
    /// Used channels with no address (one count row).
    NoAddress = "no-address", Info, "Used channels with no address";
    /// A wire that runs on through a PLC card to another point.
    WireThroughCard = "wire-through-card", Warning, "Wire runs through a PLC card";
    /// A run's texts are wider or deeper than its cells at the
    /// project's text styles (Info; nothing is shrunk).
    PlcTextWide = "plc-text-wide", Info, "PLC texts wider than their cells";
    /// A connector pin drawn twice on one half.
    ConnectorPinTwice = "connector-pin-twice", Error, "Connector pin drawn twice on one half";
    /// A connector whose whole halves are of two families.
    ConnectorFamily = "connector-family", Warning, "Connector halves of two families";
    /// A connector whose whole halves differ in size.
    ConnectorSize = "connector-size", Warning, "Connector halves of two sizes";
    /// A connector's pins with no mate (one count row per connector).
    ConnectorLonePin = "connector-lone-pin", Info, "Connector pins without a mate";
    /// A connector pin numbered beyond its `pins`.
    ConnectorOver = "connector-over", Info, "Connector pin beyond its pin count";
    /// Items without a part number (one count row).
    ItemNoPartNumber = "item-no-part-number", Info, "Items without a part number", count;
    /// Fuses (of banks, strips and fuse terminals) that no part line serves (one count row).
    FuseNoInsert = "fuse-no-insert", Info, "Fuses without an insert", count;
    /// One part number with two article numbers.
    ArticleConflict = "article-conflict", Warning, "One part number, two article numbers";
    /// A part line's `for` names a fuse or position the device does not have.
    PartForMissing = "part-for-missing", Warning, "Part line names a fuse or position the device lacks";
    /// A `part.` key, or an item's `qty` or `unit`, ECAD does not read.
    BadPartKey = "bad-part-key", Info, "Part line or item value ECAD does not read";
    /// An object's style choice names no style (drawn with the next level's).
    StyleUnknown = "style-unknown", Info, "Style not found; drawn without it";
    /// Style records past the bounds, ignored.
    StyleLimits = "style-limits", Info, "Style records past the limits are ignored";
    /// A wire's own category names no category (drawn without one).
    CategoryUnknown = "category-unknown", Info, "Wire category not found; drawn without one";
    /// Two categories name one potential (the first by ID is used).
    CategoryPotentialTwice = "category-potential-twice", Warning, "Two wire categories are the default for one potential";
    /// A category with a part number has no allowance: its BOM row
    /// has no length.
    WireAllowance = "wire-allowance", Info, "Wire length not set (set an allowance in Standards)";
    /// A page's `page.type` names no page type.
    PageTypeUnknown = "page-type-unknown", Warning, "Page type not found; drawn as its usual type";
    /// A `folder.<id>.start` names no folder or is not 1–6 digits.
    FolderStartUnknown = "folder-start-unknown", Info, "Folder start number not read";
    /// A folder's run of numbers reaches another run's.
    FolderRunsOverlap = "folder-runs-overlap", Warning, "A folder's page numbers run into another folder's";
    /// The cover's symbol legend shows fewer symbols than are
    /// used.
    CoverLegendCut = "cover-legend-cut", Info, "The cover's symbol legend does not show every symbol";
    /// Stored cover notes past the bounds, read cut.
    CoverNotesCut = "cover-notes-cut", Warning, "The cover's notes are past 40 notes of 400 characters; the rest are not shown";
    RatingUnder = "rating-under", Error, "Wire or cable rated below its protective device";
    RatingTooHot = "rating-too-hot", Error, "Conductor at or above its maximum temperature";
    RatingLoadOver = "rating-load-over", Warning, "Load above the protective device's rating";
    VoltdropOver = "voltdrop-over", Warning, "Cable volt drop over its limit";
    RatingBundlingUnset = "rating-bundling-unset", Warning, "Panel wires with no bundling factor";
    RatingDeviceMissing = "rating-device-missing", Warning, "Protected by names no device";
    RatingNoDevice = "rating-no-device", Info, "Rated wires and cables with no protective device";
    RatingTwoDevices = "rating-two-devices", Info, "Fed by two protective devices";
    RatingDeviceUnread = "rating-device-unread", Info, "Device rating has no readable current";
    RatingNamedDevice = "rating-named-device", Info, "Protected by a device that is not a protective kind";
    RatingNoData = "rating-no-data", Info, "Protected wires with no rating data";
    RatingNoTable = "rating-no-table", Info, "No rating table value for this subject";
    RatingTableBadRow = "rating-table-bad-row", Info, "Rating table rows not read";
    RatingTableOdd = "rating-table-odd", Info, "Rating table values out of order";
    RatingAssumed = "rating-assumed", Info, "Cables with no cable type (values assumed)";
    RatingNoCurrent = "rating-no-current", Info, "Cables with no load and no device (no volt drop)";
    RatingBadValue = "rating-bad-value", Info, "Rating value ECAD does not read";
    /// A cable core not connected at both ends, not ticked
    /// Not used.
    CoreOpen = "core-open", Error, "Cable core open at an end (tick Not used for a spare)";
    /// A bound value is cut in the title block on at least one page.
    TitleValueCut = "title-value-cut", Info, "A value is cut in the title block";
    /// A frame has fields this build cannot fill, or items past the bounds.
    TitleBindingUnknown = "title-binding-unknown", Info, "Title block fields this build cannot fill";
    /// A page's own sheet differs from its frame's; the frame wins.
    FrameSheetDiffers = "frame-sheet-differs", Warning, "Page sheet differs from its frame's; the frame wins";
    /// A cable built in Project ▸ Cables that is not in the drawing yet
    /// (Info; Warning in the release check).
    CableNotDrawn = "cable-not-drawn", Info, "Cable not in the drawing yet";
    /// A core on a line with one end landed: it moves onto a wire when
    /// the other end lands.
    CoreOneEnd = "core-one-end", Warning, "Core landed at one end only";
    /// A core's stored place is not where the drawing puts it (a file made elsewhere): reading never
    /// works it out, so it is reported.
    CorePlaceStale = "core-place-stale", Error, "Core's stored place disagrees with the drawing";
    /// A drawn cable end differs from that end's planned location.
    CableEndOffPlan = "cable-end-off-plan", Warning, "Cable end drawn away from its plan";
    /// A drawn cable end with no gland size.
    NoGland = "no-gland", Info, "Cable end with no gland size";
    /// A cable value ECAD does not read (out of bounds); it is ignored.
    CableBadValue = "cable-bad-value", Warning, "Cable value ECAD does not read";
    /// Two channels of one PLC share a tag (without case).
    PlcTagTwice = "plc-tag-twice", Error, "PLC tag given twice in one PLC";
    /// A tag an export leaves out (a control character, too long).
    PlcTagChars = "plc-tag-chars", Warning, "PLC tag an export leaves out";
    /// A built-in default failed to load (not reachable in a tested
    /// build); the affected entries use their kind's safe value.
    DefaultsDamaged = "defaults-damaged", Warning, "Built-in defaults damaged: safe values used";
}

impl Kind {
    /// The kind a JSON name gives.
    pub fn parse(text: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == text.trim())
    }
}

/// What a row points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Object {
    Wire(WireId),
    /// A drawn route.
    Route(RouteId),
    ConnectionPoint(ConnectionPointId),
    Device(DeviceId),
    /// A drawing aid: a location box.
    Aid(ecad_model::AidId),
    /// A cable reference end.
    Reference(ecad_model::RepresentationId),
    Cable(ecad_model::CableId),
    None,
}

/// What a fix button on a row does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fix {
    /// `joinwires {at}` on the row's page (Looks connected: **Join**).
    Join(Pt),
    /// `renumberwires {scope: "unnumbered"}`.
    NumberThem,
    /// Open Finalise numbering: the Renumber dialog, not a direct write.
    Finalise,
    /// **Use +CP1**: set the device's value of the aspect to the one
    /// context all its symbols share (`setlocation`).
    UseContext(DeviceId, crate::locations::designations::Aspect),
    /// **Set location** / **Set function**: the inspector's row of that device or box.
    SetDesignation(DeviceId, crate::locations::designations::Aspect),
    /// **Show in Settings**: Settings ▸ Project info ▸ Functions and locations.
    ShowDesignations,
    /// Open Reports on the BOM tab with Only ? on.
    ShowReports,
    /// Offer the drawn count (`setprop terminals`, PE kept): Offer 14.
    OfferDrawn(DeviceId),
    /// Renumber the strip's terminal blocks (`renumber {kind: "terminal_blocks"}`).
    RenumberStrip(DeviceId),
    /// Focus Properties ▸ Cores on the cable.
    SetCores(ecad_model::CableId),
    /// Give the member the next free core (`cablecore`).
    NextFreeCore(WireId),
    /// Open Reports on the Cables tab with Only ? on.
    ShowCables,
    /// The strip's orphan key (index into its kept orphans): **Move to end** (items
    /// appended to `acc.end`, key cleared) or **Remove** (key cleared).
    AccessoryOrphan(DeviceId, u16),
    /// Link `n` crossing the partition after position index `at`: **Split link at**
    /// (`jumper split_at`) or **Remove partition** (`accessory remove [partition]`).
    JumperBarAcrossPartition(DeviceId, u16, u16),
    /// Link `n` names missing positions: **Drop missing positions** or **Delete link**.
    JumperBarOrphan(DeviceId, u16),
    /// **Remove** a link key from a device that is not a strip.
    JumperBarOnNonStrip(DeviceId),
    /// **Move table…** starts `mirrormove` for the device.
    MoveTable(DeviceId),
    /// **Renumber…** opens this page's number field.
    RenumberPage(PageId),
    /// **Move to next free** (the level drawn twice, to the next position of its type
    /// with that level free; `terminalblocklabel {terminal, move_to: "next"}`).
    MoveToNextFree(ConnectionPointId),
    /// **Move to new position** (the other type's levels; `terminalblocklabel {terminal,
    /// move_to: "new"}`).
    MoveToNewPosition(ConnectionPointId),
    /// Link `n` names missing levels: **Drop missing levels** or **Delete link**.
    LevelMissing(DeviceId, u16),
    /// **Show** the fuse terminal devices, each with **Make strip position**
    /// (`toposition`).
    FuseDevices,
    /// A PLC row's buttons, from the kept PLC model
    /// (`crate::plc::Plc::fix_commands`).
    Plc(DeviceId, crate::plc::PlcFix),
    /// **Move to next free** (`pinlabel {terminal, to: "next"}`).
    ConnectorNextPin(ConnectionPointId),
    /// **Set pins to n** (`setprop pins`).
    SetPins(DeviceId, u32),
    /// **Place mates**: Place ▸ Connector on this connector, pin by pin, on the side
    /// the lone pins lack.
    PlaceMates(DeviceId, crate::symbols::Side),
    /// **Ignore** this warning on this connector (`setprop ignore.<kind> yes`).
    IgnoreConnector(DeviceId, Kind),
    /// **Use A-0110 for all**: the most common article number of the part of line
    /// `n` of the device, or of the item (`article`).
    UseArticle(crate::reports::Owner, u8),
    /// **Remove 9**: drop the names line `n`'s `for` lists that the device lacks
    /// (`partline set for`).
    DropMissingFor(DeviceId, u8),
    /// **Go to** Settings ▸ Standards at this catalogue (`text`, `wire`…).
    OpenStandards(ecad_model::StyleCatalogue),
    /// A page type or folder start row's fix.
    Page(crate::pages::page_types::PageFix),
    /// **Show rating**: select the subject in Project ▸ Wires or Cables, Rating unfolded.
    ShowRating(crate::circuits::RatedSubject),
    /// **Use 2.5 mm²**: the conductor's own `cross_section`.
    UseSize(WireId, f64),
    /// **Change cable type…** filtered to this size (mm²).
    PickCableType(CableId, f64),
    /// **Use Datasheet** (`rating {basis}`).
    SetBasis(crate::circuits::RatedSubject, ecad_model::objects::wire::Basis),
    /// **Clear Protected by**.
    ClearProtectedBy(crate::circuits::RatedSubject),
    /// **Choose a table**, **Set project bundling**: Settings ▸ Standards ▸ Defaults at Rating.
    ChooseRatingTable,
    /// **Show** the rated wires (Project ▸ Wires).
    ShowRatedWires,
    /// An open core, by its place in [`crate::styles::cable::open_cores`]:
    /// **Tick Not used** (one `setprop`) or **Go to the open end**.
    CoreOpen(CableId, u16),
    /// **Edit block** opens Library ▸ Title blocks on the page's frame with item `n` selected.
    EditTitleBlock(PageId, u16),
    /// **Clear the page's sheet** (`setprop` page `sheet` null).
    ClearPageSheet(PageId),
    /// **Assign in the drawing…** (the slash tool with the cable picked)
    /// or **Remove** (`cableremove`).
    AssignCable(CableId),
    /// **Use the drawing**: set that end's plan to its drawn location (one `cableedit`),
    /// or **Go to** that end.
    UseDrawing(CableId, crate::cables::Side),
    /// **Set gland** / **Show**: focus that row of the cable's Details pane.
    ShowCableField(CableId, crate::cables::CableField),
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

    /// The severity in the release check: a cable not drawn yet is a Warning
    /// there, Info elsewhere.
    pub fn release_severity(&self) -> Severity {
        match self.kind {
            Kind::CableNotDrawn => Severity::Warning,
            _ => self.severity(),
        }
    }

    /// The row's text: the kind's words, or for the unnumbered row its count. A
    /// connector row's text names its connector (`-X10's halves differ: …`).
    pub fn what(&self) -> String {
        match self.kind {
            Kind::ConnectorPinTwice
            | Kind::ConnectorFamily
            | Kind::ConnectorSize
            | Kind::ConnectorLonePin
            | Kind::ConnectorOver
            | Kind::StyleUnknown
            | Kind::StyleLimits
            | Kind::CategoryPotentialTwice
            | Kind::WireAllowance
            | Kind::PageTypeUnknown
            | Kind::FolderStartUnknown
            | Kind::FolderRunsOverlap
            | Kind::CoverLegendCut
            | Kind::CoreOpen
            | Kind::TitleValueCut
            | Kind::TitleBindingUnknown
            | Kind::FrameSheetDiffers
            | Kind::CableNotDrawn
            | Kind::CableEndOffPlan
            | Kind::NoGland
            | Kind::CableBadValue
            | Kind::DefaultsDamaged
                if !self.which.is_empty() =>
            {
                self.which.clone()
            }
            // Every rating row says its own sentence.
            k if crate::circuits::rows::is_rating_kind(k) && !self.which.is_empty() => self.which.clone(),
            _ => self.kind.words().to_string(),
        }
    }

    /// Where the row is, as references name a place (`3-D-5`, or `/3.D` in that style); the page
    /// number alone when it has no point; empty when it has no page.
    pub fn locator(&self, doc: &Document) -> String {
        let Some(page) = self.page else { return String::new() };
        match self.at.and_then(|a| ecad_model::connectivity::wiring::place(doc, page, a)) {
            Some(p) => p.text(doc.settings().reference_style()),
            None => ecad_model::document::book::page_number(doc, page),
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
            Object::Wire(c) => json!({"conductor": c.hex()}),
            Object::Route(r) => json!({"route": r.hex()}),
            Object::ConnectionPoint(t) => json!({"connection_point": t.hex()}),
            Object::Device(c) => json!({"component": c.hex()}),
            Object::Aid(a) => json!({"aid": a.hex()}),
            Object::Reference(r) => json!({"reference": r.hex()}),
            Object::Cable(k) => json!({"cable": k.hex()}),
            Object::None => Value::Null,
        };
        let place = self.page.zip(self.at).and_then(|(p, a)| ecad_model::connectivity::wiring::place(doc, p, a));
        json!({
            "kind": self.kind.name(),
            "severity": self.severity().name(),
            "page": self.page.map(|p| ecad_model::document::book::page_number(doc, p)),
            "column": place.as_ref().map(|p| p.column_label.clone()),
            "row": place.as_ref().map(|p| p.row_label.clone()),
            "at": self.at.map(|a| [a.x, a.y]),
            "object": object,
            "text": self.what(),
            "which": self.which,
            "where": self.locator(doc),
            // The command a page type or folder start fix runs.
            "fix": match self.fix {
                Some(Fix::Page(f)) => {
                    let (command, params) = f.command();
                    json!({"label": f.label(), "command": command, "params": params})
                }
                _ => Value::Null,
            },
        })
    }
}

/// What a check belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    Net(NetKey),
    Wire(WireId),
    /// A drawn route's rows (open end, loose, looks connected, drawn away).
    Route(RouteId),
    ConnectionPoint(ConnectionPointId),
    Device(DeviceId),
    /// A continuation's rows (no partner).
    Aid(ecad_model::AidId),
    /// The reference ends of a cable (orphans).
    CableReferences(CableId),
    Cable(ecad_model::CableId),
    Project,
    /// A page's dash budget.
    Page(PageId),
    /// The style rows (unknown styles, old settings, limits).
    Styles,
    /// The cover's rows (legend cut, notes cut), refreshed when the cover changes.
    Cover,
    /// A rated subject's own rows.
    Rating(crate::circuits::RatedSubject),
    /// The count rows, unread devices and the rating table's own rows.
    RatingSummary,
    /// The project's list of functions and locations (duplicate codes).
    Designations,
    /// The title block rows (cut values, unknown fields, sheets that differ).
    TitleBlocks,
}

/// An area that keeps whole-project rows under one [`Key`] (R7): it emits them from its
/// own module, and `Checks` asks it on open and whenever a commit's [`Scope`] reaches it, so a new
/// area of this kind is one implementation plus a line in [`PROJECT_SOURCES`].
pub trait CheckSource: Sync {
    /// The key its rows are kept under.
    fn key(&self) -> Key;
    /// Whether a commit with `scope` may change its rows (its update map).
    fn touched(&self, scope: &Scope) -> bool;
    /// Whether its rows stay on pages whose type has checks and references off
    /// Rows about the project's own records rather than the drawing.
    fn kept_on_pages_off(&self) -> bool {
        false
    }
    /// Its rows, from scratch.
    fn checks(&self, doc: &Document, registry: &ecad_model::registry::data::Registry, look: &dyn Look) -> Vec<Check>;
}

/// The whole-project sources, in the order `Checks::build` asks them.
pub const PROJECT_SOURCES: [&dyn CheckSource; 4] =
    [&crate::styles::checks::StyleRows, &crate::pages::front::CoverRows, &devices::DesignationRows, &crate::title_blocks::checks::TitleBlockRows];

/// The subjects a transaction could change (see the module's update map).
#[derive(Debug, Clone, Default)]
pub struct Scope {
    all: bool,
    /// Only where rows are (page numbers, folders) changed: the revision moves, so copies of the
    /// rows are taken again.
    relocate: bool,
    nodes: BTreeSet<ecad_model::Node>,
    old_nets: BTreeSet<NetKey>,
    numbers: BTreeSet<String>,
    wires: BTreeSet<WireId>,
    routes: BTreeSet<RouteId>,
    connection_points: BTreeSet<ConnectionPointId>,
    devices: BTreeSet<DeviceId>,
    /// Aids to re-check: continuations made, removed or renamed, and their
    /// partners by name; boxes made, removed or changed.
    aids: BTreeSet<ecad_model::AidId>,
    /// Cables whose reference ends to re-check (orphans).
    cable_references: BTreeSet<CableId>,
    /// Cables to re-check (provisional and shared tags).
    cables: BTreeSet<ecad_model::CableId>,
    /// The style rows (`crate::styles::checks::touches`).
    pub(crate) styles: bool,
    /// What the ops reach of the rating results.
    rating: crate::circuits::kept::Touch,
    /// The list's rows (an entry's code changed, added or removed).
    pub(crate) designations: bool,
    /// The title block rows (`crate::title_blocks::checks::touches`).
    pub(crate) title_blocks: bool,
}

impl Scope {
    /// Whether the whole list is built again: a numbering or potentials setting, or the pages with
    /// checks off, changed.
    pub fn is_all(&self) -> bool {
        self.all
    }

    /// Add what one effect names, looked up in `doc` (call once on the state before, once after).
    pub fn note(&mut self, doc: &Document, effect: &Effect<'_>) {
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
        self.rating.note(doc, effect);
        self.styles |= crate::styles::checks::touches(effect);
        self.title_blocks |= crate::title_blocks::checks::touches(effect);
        // The set of pages with checks off may change (then everything is checked
        // again).
        self.all |= crate::pages::page_types::checks_changed(doc, effect);
        match effect {
            Effect::Wire { id, before: Some(before), after: Some(after) } => {
                node(self, Node::Wire(*id));
                self.wires.insert(*id);
                self.numbers.extend([before, after].into_iter().filter_map(|w| w.number_text().map(str::to_string)));
            }
            // Made or deleted.
            Effect::Wire { id, before, after } => {
                // A member's ends decide its cable's open cores.
                self.cables.extend(doc.cables_of(*id));
                self.wires.insert(*id);
                node(self, Node::Wire(*id));
                for t in before.iter().chain(after.iter()).flat_map(|w| w.ends.iter()) {
                    self.connection_points.insert(*t);
                    node(self, Node::ConnectionPoint(*t));
                    self.component_of(doc, *t);
                }
            }
            // Added, removed, or both (a relabel).
            Effect::ConnectionPoint { id, before, after } => {
                self.connection_points.insert(*id);
                self.devices.extend(before.iter().chain(after.iter()).filter_map(|t| t.device()));
                node(self, Node::ConnectionPoint(*id));
            }
            // Another definition may bridge other points.
            Effect::DeviceDefinition { id, .. } => {
                self.devices.insert(*id);
                for (t, _) in doc.connection_points_of(*id) {
                    node(self, Node::ConnectionPoint(t));
                }
            }
            Effect::Device { id, before, after } => {
                for device in before.iter().chain(after.iter()) {
                    self.tag_holders(doc, device.tag_key());
                }
                self.component(doc, *id);
                for (t, _) in doc.connection_points_of(*id) {
                    node(self, Node::ConnectionPoint(t));
                }
                // A device's location and function are fields, where the cables
                // at its points are drawn (as the property arm below).
                if let (Some(before), Some(after)) = (before, after)
                    && (before.location != after.location || before.function != after.function)
                {
                    for (t, _) in doc.connection_points_of(*id) {
                        for (w, _) in doc.ends_at(t) {
                            self.cables.extend(doc.cables_of(w));
                        }
                    }
                }
            }
            Effect::Prop { object: ObjectRef::Device(c), key, before, after } => {
                let key = key.raw;
                if key == ecad_model::naming::tag::TAG_KEY {
                    // The op names the text only: the holders of it in either state.
                    use ecad_model::naming::tag::{Key, TagState};
                    let keys = [before, after].into_iter().flatten().flat_map(|t| [Key::of(t, TagState::Fixed), Key::of(t, TagState::Provisional)]);
                    self.tag_holders(doc, keys);
                }
                // A device's location or function is where the
                // cables at its points are drawn (`cable-end-off-plan`).
                if crate::locations::designations::Aspect::of_property(key).is_some() {
                    for (t, _) in doc.connection_points_of(*c) {
                        for (w, _) in doc.ends_at(t) {
                            self.cables.extend(doc.cables_of(w));
                        }
                    }
                }
                self.component(doc, *c);
                for (t, _) in doc.connection_points_of(*c) {
                    node(self, Node::ConnectionPoint(t));
                }
            }
            // A cable is written whole (`ModifyCable`); a property on one is refused.
            Effect::Prop { object: ObjectRef::Cable(_), .. } => {}
            Effect::Prop { object: ObjectRef::Wire(c), .. } => {
                node(self, Node::Wire(*c));
                self.wires.insert(*c);
            }
            Effect::Cable { id: k, before, after } => {
                // A new or deleted cable, or its tag (the orphan row's words).
                self.cable_references.insert(*k);
                // The cable's own rows, and every holder of the tags it had or has.
                self.cables.insert(*k);
                self.tag_holders(doc, before.iter().chain(after.iter()).filter_map(|c| c.tag_key()));
                // A cable's tag and cores are part of its members' words.
                if let Some(cable) = doc.cables().get(k) {
                    self.wires.extend(cable.cores.iter().filter_map(|m| m.wire()));
                }
                self.wires.extend(before.iter().chain(after.iter()).flat_map(|c| c.cores.iter()).filter_map(|m| m.wire()));
            }
            Effect::Item { id, .. } => self.item(doc, *id),
            Effect::Rep { before, after, .. } => {
                for r in before.iter().chain(after.iter()) {
                    self.rep(doc, r);
                }
            }
            // The route's own rows, and the points it lands on (connected or
            // not).
            // The commit's rewrite pass makes and removes wires at those points (and at the
            // points of a route it lands on), which the ops given to `scope` do not hold yet:
            // their nets as they were lose their rows.
            Effect::Route { id, before, after } => {
                self.routes.insert(*id);
                // The cables with a core on it: their core rows.
                self.cables.extend(doc.cables_on_route(*id));
                for t in before.iter().chain(after.iter()).flat_map(|r| cables::route_points(doc, r)) {
                    self.connection_points.insert(t);
                    node(self, Node::ConnectionPoint(t));
                }
            }
            Effect::Definition { id, .. } => {
                for c in doc.components_using(*id) {
                    self.devices.insert(c);
                    for (t, _) in doc.connection_points_of(c) {
                        self.connection_points.insert(t);
                        node(self, Node::ConnectionPoint(t));
                    }
                }
            }
            // Only the numbering and potential settings change what checks find; sheet, page
            // numbering and reference style change where rows say they are; label looks
            // (colour style, height), tag settings and units change nothing here.
            Effect::Settings { before, after } => {
                // A table the user placed may enter or leave the
                // title block when the project's sheet changes.
                if before.sheet != after.sheet {
                    self.devices.extend(crate::references::devices::read::placed_tables(doc));
                }
                let (b, a) = (&before.wiring, &after.wiring);
                if b.scheme != a.scheme || b.digits != a.digits || b.unclassified_prefix != a.unclassified_prefix || b.potentials != a.potentials {
                    self.all = true;
                } else if before.sheet != after.sheet
                    || before.page_number_step != after.page_number_step
                    || before.reference_style != after.reference_style
                {
                    self.relocate = true;
                }
            }
            // A page's sheet moves its frame, so the devices on it
            // are checked again for a table over the title block.
            // A page changed in place; its sheet moves its frame (as below), its
            // function or location is a context for the devices on it.
            Effect::Page { id: p, before: Some(before), after: Some(after), index_before: None, index_after: None } => {
                let ch = before.change(after);
                self.relocate = true;
                if ch.sheet {
                    for (_, r) in doc.representations_on(*p) {
                        if let Subject::Device(c) = r.subject {
                            self.devices.insert(c);
                        }
                    }
                }
                if ch.location || ch.function {
                    self.devices_on(doc, *p);
                }
            }
            // A removed page's reference ends go with it (a page moved too: it is
            // removed and added).
            Effect::Page { id, before: Some(_), .. } => {
                self.relocate = true;
                for (_, r) in doc.representations_on(*id) {
                    if let Subject::Cable(k) = r.subject {
                        self.cable_references.insert(k);
                    }
                }
            }
            // Page numbers, folders and the page order change where rows say they are (brief
            // not what they find: the rows are copied again.
            Effect::Page { before: None, .. }
            | Effect::Folder { .. }
            | Effect::Prop { object: ObjectRef::Page(_) | ObjectRef::Project | ObjectRef::Standard, .. } => self.relocate = true,
            // No check or report reads a device link yet.
            Effect::DeviceLink { .. } => {}
            // A box's values are a context for the devices on its pages; a
            // continuation's names join nets.
            Effect::Aid { id, before, after } => {
                self.aid(doc, *id, before.iter().chain(after.iter()).copied());
                for n in before.iter().chain(after.iter()).filter_map(|a| a.continuation_name()) {
                    node(self, Node::Name(name_key(n.as_str())));
                }
                // Made or deleted: its points.
                if before.is_none() || after.is_none() {
                    for (t, _) in doc.connection_points_of(*id) {
                        self.connection_points.insert(t);
                        node(self, Node::ConnectionPoint(t));
                    }
                }
            }
            // A loose part's checks (an item with no part number) are the BOM's,
            // kept by the report models (`ReportModels::bom_checks`); no drawing check reads one.
            Effect::LoosePart { .. } => {}
            // The project's default context for every device (rare);
            // the list's codes for the objects storing that entry, and the duplicate rows.
            Effect::Project { before, after } => {
                use crate::locations::designations::{Aspect, changed_codes, default_key};
                for a in Aspect::ALL {
                    if default_key(before, a) != default_key(after, a) {
                        self.relocate = true;
                        self.devices.extend(doc.devices().keys().copied());
                    }
                    let changed = changed_codes(before, after, a);
                    if !changed.is_empty() {
                        self.relocate = true;
                        self.designations = true;
                    }
                    for entry in &changed {
                        // The cables planning an end there (their rows name its code).
                        self.cables.extend(crate::cables::planning(doc, entry));
                        for (c, x) in doc.devices() {
                            if crate::locations::designations::device_keys(x).iter().any(|(_, v)| v == entry) {
                                self.devices.insert(*c);
                            }
                        }
                        for (b, x) in doc.aids() {
                            if x.as_box().is_some_and(|x| crate::locations::designations::box_key(x, a).as_deref() == Some(entry.as_str())) {
                                self.aids.insert(*b);
                            }
                        }
                    }
                }
            }
            Effect::Layer { .. } | Effect::Version { .. } => {}
            // A cable type's cores and gland reach its users' rows.
            Effect::Style { catalogue: ecad_model::StyleCatalogue::Cable, id, .. } => {
                self.cables.extend(crate::styles::cable::users(doc, id));
            }
            // Style records and the standard reach only the style rows (above).
            Effect::Style { .. } | Effect::Standard { .. } => {}
        }
    }

    /// A drawing aid made, removed or changed: a box's pages' devices (their contexts); a
    /// continuation and its partners by its names.
    fn aid<'a>(&mut self, doc: &Document, id: ecad_model::AidId, aids: impl IntoIterator<Item = &'a ecad_model::Aid>) {
        self.aids.insert(id);
        let aids: Vec<&ecad_model::Aid> = aids.into_iter().collect();
        for k in aids.iter().filter_map(|a| ecad_model::connectivity::continuation_key(a)) {
            self.aids.extend(doc.continuations_named(&k));
        }
        if aids.iter().any(|a| a.as_box().is_some()) {
            for p in doc.representations_of(Subject::Aid(id)).map(|(_, r)| r.page).collect::<Vec<_>>() {
                self.devices_on(doc, p);
            }
        }
    }

    /// Every device with a symbol on `page` (their contexts may have changed).
    fn devices_on(&mut self, doc: &Document, page: PageId) {
        for (_, r) in doc.representations_on(page) {
            if let Subject::Device(c) = r.subject
                && crate::locations::is_device(doc, c)
            {
                self.devices.insert(c);
            }
        }
    }

    /// Every device and cable holding one of `tags` (a shared tag's rows on both sides).
    fn tag_holders(&mut self, doc: &Document, tags: impl IntoIterator<Item = ecad_model::naming::tag::Key>) {
        for k in tags {
            self.devices.extend(doc.components_with(&k));
            self.cables.extend(doc.cables_with(&k));
        }
    }

    fn component_of(&mut self, doc: &Document, t: ConnectionPointId) {
        if let Some(c) = doc.connection_points().get(&t).and_then(|x| x.device()) {
            self.devices.insert(c);
        }
    }

    /// A component and its points.
    fn component(&mut self, doc: &Document, c: DeviceId) {
        self.devices.insert(c);
        for (t, _) in doc.connection_points_of(c) {
            self.connection_points.insert(t);
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
            Subject::Wire(c) => {
                self.wires.insert(c);
            }
            Subject::Route(x) => {
                self.routes.insert(x);
            }
            // A box drawn, moved or erased changes the contexts on its page.
            // A continuation's place moves its own and its partners' rows.
            Subject::Aid(a) => {
                if let Some(x) = doc.aids().get(&a) {
                    self.aid(doc, a, [x]);
                }
            }
            Subject::Device(c) => {
                // Its points moved: the routes landing on them may now be drawn away.
                self.component(doc, c);
                for (t, _) in doc.connection_points_of(c) {
                    self.routes.extend(doc.routes_at(t));
                }
            }
            // A cable's drawing moves its rows; a reference end linked, changed or
            // unlinked (its point an item op) re-derives its orphans.
            Subject::Cable(k) => {
                self.cables.insert(k);
                self.cable_references.insert(k);
            }
        }
    }
}

/// `rows` in book order.
pub fn sort(doc: &Document, mut rows: Vec<Check>) -> Vec<Check> {
    let order: HashMap<PageId, usize> = doc.page_order().iter().enumerate().map(|(i, p)| (*p, i)).collect();
    rows.sort_by_key(|c| c.order_key(&order));
    rows
}

/// Every check of the project with the report models already kept, measuring text with `m`
/// (ecad-engine's `crate::checks::all` and `crate::checks::all_with` pass CADCraft's fonts).
pub fn all_measured(
    doc: &Document, tol: f64, reports: &crate::reports::kept::ReportModels, m: &dyn Look, registry: &ecad_model::registry::data::Registry,
) -> Vec<Check> {
    let mut rows: Vec<Check> = kept::Checks::build_with(doc, tol, m, registry).rows().cloned().collect();
    rows.extend(reports.checks());
    sort(doc, rows)
}

// The engine half, at its old path until step 6 turns this into a facade.
