//! The Checks list (brief 022, design 022 §8): open ends, loose wires, false-looking connections,
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
//! # Update map (design 022 §11)
//! | Op | Re-checked |
//! | --- | --- |
//! | `CreateConductor` / `DeleteConductor` / `AttachEnd` | that conductor; the connection points at its ends before and after; their nets (before and after) and the nets sharing their numbers |
//! | `AddTerminal` / `RemoveTerminal` | that point and its net |
//! | `CreateDevice` / `DeleteDevice` / `SetProperty` component | that component (its contact count against its part's `contacts`, brief 025); its points' nets; continuations with its name (partners) |
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

use ecad_model::numbering::Places;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use ecad_model::connectivity::{CONTINUATION, JUNCTION, NAME, name_key};
use ecad_model::wiring;
use ecad_model::{CableId, ConductorId, DeviceId, Document, End, NetData, NetId, ObjectRef, Op, PageId, Pt, Subject, TerminalId};
use serde_json::{Value, json};

use crate::text_style::fonts::TextMetrics;
use crate::wires::{self, ON_WIRE, kind_of};

pub mod kept;

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

/// One row per check kind (brief 060, R7; review 2026-11, finding 5): its variant, its JSON name
/// (persisted: never renamed), its severity, the words of the Show list and, after them, `count`
/// for a row that shows a count rather than a place (`12 wires`). Generates [`Kind`],
/// [`Kind::ALL`] (in table order), [`Kind::name`], [`Kind::severity`], [`Kind::words`] and
/// [`Kind::is_count`]; a new check is one row here plus its source module.
macro_rules! check_kinds {
    ($($(#[$doc:meta])* $kind:ident = $name:literal, $severity:ident, $words:expr $(, $count:ident)?;)*) => {
        /// What a check found (design 022 §8's table, in its order).
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
    LooksConnected = "looks-connected", Warning, looks_connected_words();
    DrawnAway = "drawn-away", Warning, "End drawn away from its connection point";
    SinglePoint = "single-point", Warning, "Net with a single connection point";
    ThinJunction = "thin-junction", Info, "Junction with fewer than three wires";
    Unconnected = "unconnected", Info, "Connection point not connected";
    LonelyContinuation = "lonely-continuation", Info, "Continuation without partner";
    MixedPotentials = "mixed-potentials", Info, "Mixed potentials in one net";
    Unnumbered = "unnumbered", Info, "Unnumbered nets", count;
    UnknownName = "unknown-name", Warning, "Continuation name not a potential";
    /// Two cables, or a cable and a device, share a tag (brief 026: one namespace).
    DuplicateTag = "duplicate-tag", Error, "Two cables share a tag";
    /// A provisional tag (brief 026, design §5.4).
    ProvisionalTag = "provisional-tag", Warning, "Provisional tag";
    /// Nets without a number while provisional tags are on: one summary row (Q6).
    ProvisionalWires = "provisional-wires", Warning, "Wire numbers provisional", count;
    /// Brief 052 (design 052 §6): a device's stored location is the context of none of its
    /// symbols.
    LocationUnused = "location-unused", Warning, "Location used by none of its symbols";
    /// The same for its function.
    FunctionUnused = "function-unused", Warning, "Function used by none of its symbols";
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
    /// A device with more contacts of a kind drawn than its part offers (brief 025).
    TooManyContacts = "too-many-contacts", Warning, "More contacts than the part offers";
    /// Brief 028 (design 028 §8): a strip with more terminals drawn than it offers.
    TooManyTerminals = "too-many-terminals", Warning, "More terminals than the strip offers";
    /// Two terminals of one strip with one label.
    TerminalTwice = "terminal-twice", Error, "Terminal drawn twice";
    /// A cable with more members than cores offered.
    TooManyCores = "too-many-cores", Warning, "More cores used than the cable offers";
    /// Two members of one cable with one core label.
    CoreTwice = "core-twice", Warning, "Core used by two wires";
    /// Cables with no core count (one count row).
    NoCoreCount = "no-core-count", Info, "Cables without a core count", count;
    /// A cable reference end with no other end (brief 027, design 027 §4.2).
    OrphanReference = "orphan-reference", Warning, "Cable reference with no other end";
    /// Brief 030 (design 030 §8): an accessory placed after or on a position the strip lacks.
    AccessoryOrphan = "accessory-orphan", Warning, "Accessory at a position the strip lacks";
    /// Placed strip accessories without a part number (one count row).
    NoAccessoryPart = "no-accessory-part", Info, "Strip accessories without a part number", count;
    /// A link spanning a partition.
    LinkAcrossPartition = "link-across-partition", Warning, "Link crosses a partition";
    /// A link naming positions the strip does not have.
    LinkOrphan = "link-orphan", Warning, "Link names positions the strip lacks";
    /// A `link.<n>` key on a device that is not a terminal strip (review of PR #57, 4).
    LinkOnNonStrip = "link-on-non-strip", Warning, "Link on a device that is not a strip";
    /// Brief 033: a contact table the user placed overlaps the title block.
    TableOverTitle = "table-over-title", Warning, "Contact table over the title block";
    /// Brief 033 (design 033 §5.3): a generated page grew onto another page's number.
    ContinuationClash = "continuation-clash", Warning, "Continuation sheet has another page's number";
    /// Brief 036 (design 036 §8): a level of a multi-level position drawn twice.
    LevelTwice = "level-twice", Error, "Terminal level drawn twice";
    /// A position whose levels are drawn by symbols of two types.
    PositionMixed = "position-mixed", Warning, "Position mixes terminal types";
    /// A link naming a level its position does not have.
    LevelMissing = "level-missing", Warning, "Link names a level the position lacks";
    /// Fuse terminals that are devices of their own (one count row).
    FuseDevice = "fuse-device", Info, "Fuse terminals that are devices of their own";
    /// Fuse positions with no rating (one count row).
    NoFuseRating = "no-fuse-rating", Info, "Fuse positions without a rating";
    /// Brief 040: a wire or core colour code the app does not know (drawn in ink).
    UnknownColour = "unknown-colour", Info, "Colour code not defined (drawn in ink); define it in Standards";
    /// Brief 040: a page with more two-colour dashes than it draws (one row per page).
    DashesLeftOut = "dashes-left-out", Warning, "Two-colour wires drawn in one colour (too many dashes on the page)";
    /// Brief 045 (design 045 §9): two cards in one slot of a rack.
    SlotTwice = "slot-twice", Error, "Two cards in one slot";
    /// A card naming a rack that is not in the project.
    RackMissing = "rack-missing", Warning, "Card names a rack that is not in the project";
    /// A card in a slot its rack does not have (or in a rack but no slot).
    SlotRange = "slot-range", Warning, "Card in a slot its rack does not have";
    /// PLC cards in no rack (one count row).
    Unracked = "unracked", Info, "PLC cards in no rack", count;
    /// Brief 045b: one channel drawn twice.
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
    /// Brief 051 (design 051 §2.8): a wire that runs on through a PLC card to another point.
    WireThroughCard = "wire-through-card", Warning, "Wire runs through a PLC card";
    /// Brief 051 (design 051 §2.5): a run's texts are wider or deeper than its cells at the
    /// project's text styles (Info; nothing is shrunk).
    PlcTextWide = "plc-text-wide", Info, "PLC texts wider than their cells";
    /// Brief 044 (design 044 §6.3): a connector pin drawn twice on one half.
    ConnectorPinTwice = "connector-pin-twice", Error, "Connector pin drawn twice on one half";
    /// A connector whose whole halves are of two families.
    ConnectorFamily = "connector-family", Warning, "Connector halves of two families";
    /// A connector whose whole halves differ in size.
    ConnectorSize = "connector-size", Warning, "Connector halves of two sizes";
    /// A connector's pins with no mate (one count row per connector).
    ConnectorLonePin = "connector-lone-pin", Info, "Connector pins without a mate";
    /// A connector pin numbered beyond its `pins`.
    ConnectorOver = "connector-over", Info, "Connector pin beyond its pin count";
    /// Brief 047 (design 047 §9): items without a part number (one count row).
    ItemNoPartNumber = "item-no-part-number", Info, "Items without a part number", count;
    /// Fuses (of banks, strips and fuse terminals) that no part line serves (one count row).
    FuseNoInsert = "fuse-no-insert", Info, "Fuses without an insert", count;
    /// One part number with two article numbers.
    ArticleConflict = "article-conflict", Warning, "One part number, two article numbers";
    /// A part line's `for` names a fuse or position the device does not have.
    PartForMissing = "part-for-missing", Warning, "Part line names a fuse or position the device lacks";
    /// A `part.` key, or an item's `qty` or `unit`, ECAD does not read.
    BadPartKey = "bad-part-key", Info, "Part line or item value ECAD does not read";
    /// Brief 050a: an object's style choice names no style (drawn with the next level's).
    StyleUnknown = "style-unknown", Info, "Style not found; drawn without it";
    /// Brief 050a (FP4 ruling): 031's text sizes, weights or fonts set as project properties in
    /// an older build, no longer read. One row per kind of key.
    IgnoredOldSetting = "ignored-old-setting", Info, "Set in an older build, now ignored: set it in Standards";
    /// Brief 050a (design 050 §3.2): style records past the bounds, ignored.
    StyleLimits = "style-limits", Info, "Style records past the limits are ignored";
    /// Brief 050b: a wire's own category names no category (drawn without one).
    CategoryUnknown = "category-unknown", Info, "Wire category not found; drawn without one";
    /// Brief 050b: two categories name one potential (the first by ID is used).
    CategoryPotentialTwice = "category-potential-twice", Warning, "Two wire categories are the default for one potential";
    /// Brief 050b (design 050 §5.6): a category with a part number has no allowance: its BOM row
    /// has no length.
    WireAllowance = "wire-allowance", Info, "Wire length not set (set an allowance in Standards)";
    /// Brief 054 (design 054 §9): a page's `page.type` names no page type.
    PageTypeUnknown = "page-type-unknown", Warning, "Page type not found; drawn as its usual type";
    /// Brief 054: a page type record holds a value its field does not take.
    PageTypeField = "page-type-field", Warning, "Page type value not read; its usual value is used";
    /// Brief 054: a `folder.<id>.start` names no folder or is not 1–6 digits.
    FolderStartUnknown = "folder-start-unknown", Info, "Folder start number not read";
    /// Brief 054 (design 054 §4.4): a folder's run of numbers reaches another run's.
    FolderRunsOverlap = "folder-runs-overlap", Warning, "A folder's page numbers run into another folder's";
    /// Brief 054b (design 054 §6.3, Q6): the cover's symbol legend shows fewer symbols than are
    /// used.
    CoverLegendCut = "cover-legend-cut", Info, "The cover's symbol legend does not show every symbol";
    /// Brief 054b (design 054 §6.4): stored cover notes past the bounds, read cut.
    CoverNotesCut = "cover-notes-cut", Warning, "The cover's notes are past 40 notes of 400 characters; the rest are not shown";
    /// Brief 057 (design 057 §7).
    RatingUnder = "rating-under", Error, "Wire or cable rated below its protective device";
    /// Brief 057 (design 057 §7).
    RatingTooHot = "rating-too-hot", Error, "Conductor at or above its maximum temperature";
    /// Brief 057 (design 057 §7).
    RatingLoadOver = "rating-load-over", Warning, "Load above the protective device's rating";
    /// Brief 057 (design 057 §7).
    VoltdropOver = "voltdrop-over", Warning, "Cable volt drop over its limit";
    /// Brief 057 (design 057 §7).
    RatingBundlingUnset = "rating-bundling-unset", Warning, "Panel wires with no bundling factor";
    /// Brief 057 (design 057 §7).
    RatingDeviceMissing = "rating-device-missing", Warning, "Protected by names no device";
    /// Brief 057 (design 057 §7).
    RatingNoDevice = "rating-no-device", Info, "Rated wires and cables with no protective device";
    /// Brief 057 (design 057 §7).
    RatingTwoDevices = "rating-two-devices", Info, "Fed by two protective devices";
    /// Brief 057 (design 057 §7).
    RatingDeviceUnread = "rating-device-unread", Info, "Device rating has no readable current";
    /// Brief 057 (design 057 §7).
    RatingNamedDevice = "rating-named-device", Info, "Protected by a device that is not a protective kind";
    /// Brief 057 (design 057 §7).
    RatingNoData = "rating-no-data", Info, "Protected wires with no rating data";
    /// Brief 057 (design 057 §7).
    RatingNoTable = "rating-no-table", Info, "No rating table value for this subject";
    /// Brief 057 (design 057 §7).
    RatingTableBadRow = "rating-table-bad-row", Info, "Rating table rows not read";
    /// Brief 057 (design 057 §7).
    RatingTableOdd = "rating-table-odd", Info, "Rating table values out of order";
    /// Brief 057 (design 057 §7).
    RatingAssumed = "rating-assumed", Info, "Cables with no cable type (values assumed)";
    /// Brief 057 (design 057 §7).
    RatingNoCurrent = "rating-no-current", Info, "Cables with no load and no device (no volt drop)";
    /// Brief 057 (design 057 §7).
    RatingBadValue = "rating-bad-value", Info, "Rating value ECAD does not read";
    /// Brief 057 (owner rule, board 2.06): a cable core not connected at both ends, not ticked
    /// Not used.
    CoreOpen = "core-open", Error, "Cable core open at an end (tick Not used for a spare)";
    /// Brief 055 (design 055 §8): a bound value is cut in the title block on at least one page.
    TitleValueCut = "title-value-cut", Info, "A value is cut in the title block";
    /// Brief 055: a frame has fields this build cannot fill, or items past the bounds.
    TitleBindingUnknown = "title-binding-unknown", Info, "Title block fields this build cannot fill";
    /// Brief 055: a page's own sheet differs from its frame's; the frame wins.
    FrameSheetDiffers = "frame-sheet-differs", Warning, "Page sheet differs from its frame's; the frame wins";
    /// Brief 059 (design 059 §6): a cable built in Project ▸ Cables that is not in the drawing yet
    /// (Info; Warning in the release check).
    CableNotDrawn = "cable-not-drawn", Info, "Cable not in the drawing yet";
    /// Brief 059: a drawn cable end differs from that end's planned location.
    CableEndOffPlan = "cable-end-off-plan", Warning, "Cable end drawn away from its plan";
    /// Brief 059 (059d): a drawn cable end with no gland size.
    NoGland = "no-gland", Info, "Cable end with no gland size";
    /// Brief 059: a cable value ECAD does not read (out of bounds); it is ignored.
    CableBadValue = "cable-bad-value", Warning, "Cable value ECAD does not read";
    /// Brief 056 (design 056 §9): two channels of one PLC share a tag (without case).
    PlcTagTwice = "plc-tag-twice", Error, "PLC tag given twice in one PLC";
    /// A tag an export leaves out (a control character, too long).
    PlcTagChars = "plc-tag-chars", Warning, "PLC tag an export leaves out";
    /// Brief 066 (design 066 §7): a built-in default failed to load (not reachable in a tested
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
    Conductor(ConductorId),
    Terminal(TerminalId),
    Device(DeviceId),
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
    /// Brief 052 (design 052 §6): **Use +CP1**: set the device's value of the aspect to the one
    /// context all its symbols share (`setlocation`).
    UseContext(DeviceId, crate::designations::Aspect),
    /// Brief 052: **Set location** / **Set function**: the inspector's row of that device or box.
    SetDesignation(DeviceId, crate::designations::Aspect),
    /// Brief 052: **Show in Settings**: Settings ▸ Project info ▸ Functions and locations.
    ShowDesignations,
    /// Brief 024: open Reports on the BOM tab with Only ? on.
    ShowReports,
    /// Brief 028: offer the drawn count (`setprop terminals`, PE kept): Offer 14.
    OfferDrawn(DeviceId),
    /// Brief 028: renumber the strip's terminals (`renumber {kind: "terminals"}`).
    RenumberStrip(DeviceId),
    /// Brief 028: focus Properties ▸ Cores on the cable.
    SetCores(ecad_model::CableId),
    /// Brief 028: give the member the next free core (`cablecore`).
    NextFreeCore(ConductorId),
    /// Brief 028: open Reports on the Cables tab with Only ? on.
    ShowCables,
    /// Brief 030: the strip's orphan key (index into its kept orphans): **Move to end** (items
    /// appended to `acc.end`, key cleared) or **Remove** (key cleared).
    AccessoryOrphan(DeviceId, u16),
    /// Brief 030: link `n` crossing the partition after position index `at`: **Split link at**
    /// (`jumper split_at`) or **Remove partition** (`accessory remove [partition]`).
    LinkAcrossPartition(DeviceId, u16, u16),
    /// Brief 030: link `n` names missing positions: **Drop missing positions** or **Delete link**.
    LinkOrphan(DeviceId, u16),
    /// Brief 030 (review of PR #57, 4): **Remove** a link key from a device that is not a strip.
    LinkOnNonStrip(DeviceId),
    /// Brief 033 (design 033 §4.1): **Move table…** starts `mirrormove` for the device.
    MoveTable(DeviceId),
    /// Brief 033 (design 033 §5.3): **Renumber…** opens this page's number field.
    RenumberPage(PageId),
    /// Brief 036: **Move to next free** (the level drawn twice, to the next position of its type
    /// with that level free; `termlabel {terminal, move_to: "next"}`).
    MoveToNextFree(TerminalId),
    /// Brief 036: **Move to new position** (the other type's levels; `termlabel {terminal,
    /// move_to: "new"}`).
    MoveToNewPosition(TerminalId),
    /// Brief 036: link `n` names missing levels: **Drop missing levels** or **Delete link**.
    LevelMissing(DeviceId, u16),
    /// Brief 036: **Show** the fuse terminal devices, each with **Make strip position**
    /// (`toposition`).
    FuseDevices,
    /// Brief 045 (design 045 §9): a PLC row's buttons, from the kept PLC model
    /// (`crate::plc::Plc::fix_commands`).
    Plc(DeviceId, crate::plc::PlcFix),
    /// Brief 044: **Move to next free** (`pinlabel {terminal, to: "next"}`).
    ConnectorNextPin(TerminalId),
    /// Brief 044: **Set pins to n** (`setprop pins`).
    SetPins(DeviceId, u32),
    /// Brief 044: **Place mates**: Place ▸ Connector on this connector, pin by pin, on the side
    /// the lone pins lack (§4.4 gives them first).
    PlaceMates(DeviceId, crate::symbols::Side),
    /// Brief 044: **Ignore** this warning on this connector (`setprop ignore.<kind> yes`).
    IgnoreConnector(DeviceId, Kind),
    /// Brief 047: **Use A-0110 for all**: the most common article number of the part of line
    /// `n` of the component (`article`).
    UseArticle(DeviceId, u8),
    /// Brief 047: **Remove 9**: drop the names line `n`'s `for` lists that the device lacks
    /// (`partline set for`).
    DropMissingFor(DeviceId, u8),
    /// Brief 050a: **Go to** Settings ▸ Standards at this catalogue (`text`, `wire`…).
    OpenStandards(&'static str),
    /// Brief 054 (design 054 §9): a page type or folder start row's fix.
    Page(crate::page_types::PageFix),
    /// Brief 057: **Show rating**: select the subject in Project ▸ Wires or Cables, Rating unfolded.
    ShowRating(crate::rating::RatedSubject),
    /// Brief 057: **Use 2.5 mm²**: the conductor's own `cross_section`.
    UseSize(ConductorId, f64),
    /// Brief 057: **Change cable type…** filtered to this size (mm²).
    PickCableType(CableId, f64),
    /// Brief 057: **Use Datasheet** (`rating {basis}`).
    SetBasis(crate::rating::RatedSubject, crate::rating::Basis),
    /// Brief 057: **Clear Protected by**.
    ClearProtectedBy(crate::rating::RatedSubject),
    /// Brief 057: **Choose a table**, **Set project bundling**: Settings ▸ Standards ▸ Defaults at Rating.
    ChooseRatingTable,
    /// Brief 057: **Show** the rated wires (Project ▸ Wires).
    ShowRatedWires,
    /// Brief 057 (owner): an open core, by its place in [`crate::styles::cable::open_cores`]:
    /// **Tick Not used** (one `setprop`) or **Go to the open end**.
    CoreOpen(CableId, u16),
    /// Brief 055: **Edit block** opens Library ▸ Title blocks on the page's frame with item `n` selected.
    EditTitleBlock(PageId, u16),
    /// Brief 055: **Clear the page's sheet** (`setprop` page `sheet` null).
    ClearPageSheet(PageId),
    /// Brief 059 (design 059 §6): **Assign in the drawing…** (the slash tool with the cable picked)
    /// or **Remove** (`cableremove`).
    AssignCable(CableId),
    /// Brief 059: **Use the drawing**: set that end's plan to its drawn location (one `cableedit`),
    /// or **Go to** that end.
    UseDrawing(CableId, crate::cables::Side),
    /// Brief 059: **Set gland** / **Show**: focus that row of the cable's Details pane.
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

    /// The severity in the release check (029; brief 059, Q2): a cable not drawn yet is a Warning
    /// there, Info elsewhere.
    pub fn release_severity(&self) -> Severity {
        match self.kind {
            Kind::CableNotDrawn => Severity::Warning,
            _ => self.severity(),
        }
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
            | Kind::PageTypeUnknown
            | Kind::PageTypeField
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
            // Brief 057: every rating row says its own sentence.
            k if crate::rating::is_rating_kind(k) && !self.which.is_empty() => self.which.clone(),
            _ => self.kind.words().to_string(),
        }
    }

    /// Where the row is, as references name a place (`3-D-5`, or `/3.D` in that style); the page
    /// number alone when it has no point; empty when it has no page.
    pub fn locator(&self, doc: &Document) -> String {
        let Some(page) = self.page else { return String::new() };
        match self.at.and_then(|a| ecad_model::wiring::place(doc, page, a)) {
            Some(p) => p.text(doc.settings.reference_style()),
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
            Object::Device(c) => json!({"component": c.hex()}),
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
            // Brief 054: the command a page type or folder start fix runs.
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
    Net(NetId),
    Conductor(ConductorId),
    Terminal(TerminalId),
    Device(DeviceId),
    /// The reference ends of a cable (brief 027: orphans).
    CableReferences(CableId),
    Cable(ecad_model::CableId),
    Project,
    /// Brief 040: a page's dash budget.
    Page(PageId),
    /// Brief 050a: the style rows (unknown styles, old settings, limits).
    Styles,
    /// Brief 054b: the cover's rows (legend cut, notes cut), refreshed when the cover changes.
    Cover,
    /// Brief 057: a rated subject's own rows.
    Rating(crate::rating::RatedSubject),
    /// Brief 057: the count rows, unread devices and the rating table's own rows.
    RatingSummary,
    /// Brief 052: the project's list of functions and locations (duplicate codes).
    Designations,
    /// Brief 055: the title block rows (cut values, unknown fields, sheets that differ).
    TitleBlocks,
}

/// An area that keeps whole-project rows under one [`Key`] (brief 060, R7): it emits them from its
/// own module, and `Checks` asks it on open and whenever a commit's [`Scope`] reaches it, so a new
/// area of this kind is one implementation plus a line in [`PROJECT_SOURCES`].
pub trait CheckSource: Sync {
    /// The key its rows are kept under.
    fn key(&self) -> Key;
    /// Whether a commit with `scope` may change its rows (its update map).
    fn touched(&self, scope: &Scope) -> bool;
    /// Whether its rows stay on pages whose type has checks and references off (design 054
    /// §3.4): rows about the project's own records rather than the drawing.
    fn kept_on_pages_off(&self) -> bool {
        false
    }
    /// Its rows, from scratch.
    fn checks(&self, doc: &Document) -> Vec<Check>;
}

/// The whole-project sources, in the order `Checks::build` asks them.
pub const PROJECT_SOURCES: [&dyn CheckSource; 4] =
    [&crate::styles::checks::StyleRows, &crate::front::CoverRows, &DesignationRows, &crate::title_block::checks::TitleBlockRows];

/// Brief 052: the project's list of functions and locations ([`designation_rows`]).
pub struct DesignationRows;

impl CheckSource for DesignationRows {
    fn key(&self) -> Key {
        Key::Designations
    }
    fn touched(&self, scope: &Scope) -> bool {
        scope.designations
    }
    fn checks(&self, doc: &Document) -> Vec<Check> {
        designation_rows(doc)
    }
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
pub fn conductor_place(doc: &Document, c: ConductorId, end: Option<End>) -> (Option<PageId>, Option<Pt>) {
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
        let shown = doc.representations_of(Subject::Device(c)).any(|(_, r)| r.page == *page);
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
                (p, a, doc.terminals.get(t).map_or(Object::None, |x| Object::Device(x.component)))
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
            let c = doc.devices.get(&x.component)?;
            (ecad_model::connectivity::continuation_key(&c.properties).as_ref() == Some(key)).then_some((*t, x.component))
        });
        let (p, a, o, which) = match cont {
            Some((t, c)) => {
                let (p, a) = terminal_place(doc, t);
                let name = doc.devices.get(&c).and_then(|x| x.properties.get(NAME)).cloned();
                (p, a, Object::Device(c), name.unwrap_or_else(|| key.clone()))
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
    if let Some(code) = crate::words::conductor_colour(doc, c).filter(|code| crate::words::colour_code(code).is_none()) {
        let (page, at) = conductor_place(doc, c, None);
        let code = crate::words::shorten(code.trim(), crate::words::MAX_CODE_CHARS);
        out.push(Check { kind: Kind::UnknownColour, page, at, object: Object::Conductor(c), which: format!("{which} {code}"), fix: None });
    }
    out
}

/// The checks of `page` as a whole: brief 040's dash budget ([`crate::words::dash_budget`]).
pub fn page_checks(doc: &Document, page: PageId) -> Vec<Check> {
    let left = crate::words::dash_budget(doc, page).left_out;
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
    doc.representations_of(Subject::Device(c)).any(|(_, r)| {
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
pub fn component_checks(doc: &Document, c: DeviceId, m: &dyn TextMetrics) -> Vec<Check> {
    component_checks_in(&mut Places::new(doc), c, m)
}

/// As [`component_checks`], reading the book and grids through `places`, shared across a Checks
/// pass so the book is built at most once (review of #105, 1).
pub fn component_checks_in(places: &mut Places<'_>, c: DeviceId, m: &dyn TextMetrics) -> Vec<Check> {
    let doc = places.doc();
    let mut out = Vec::new();
    designation_checks(places, c, &mut out);
    let place = || doc.terminals_of(c).next().map_or((None, None), |(t, _)| terminal_place(doc, t));
    match kind_of(doc, c) {
        Some(JUNCTION) => {
            let wires_here: usize = doc.terminals_of(c).map(|(t, _)| doc.ends_at(t).len()).sum();
            if wires_here < 3 {
                let (page, at) = place();
                out.push(Check { kind: Kind::ThinJunction, page, at, object: Object::Device(c), which: format!("{wires_here} wires"), fix: None });
            }
        }
        Some(CONTINUATION) => {
            let name = doc.devices.get(&c).and_then(|x| x.properties.get(NAME)).cloned().unwrap_or_default();
            let partners = doc.continuations_named(&name_key(&name)).len();
            if partners < 2 {
                let (page, at) = place();
                out.push(Check { kind: Kind::LonelyContinuation, page, at, object: Object::Device(c), which: name, fix: None });
            }
        }
        _ => {
            // Brief 026: a provisional tag, and a tag a cable holds too.
            if let Some(t) = crate::devices::tag_of(doc, c)
                && (ecad_model::tag::is_provisional(t) || !doc.cables_tagged(t).is_empty())
            {
                // Located only when there is a row to show (a build visits every device).
                let (page, at) = crate::reps::goto_place_in(places, c).map_or((None, None), |(p, a)| (Some(p), a));
                if ecad_model::tag::is_provisional(t) {
                    out.push(Check {
                        kind: Kind::ProvisionalTag,
                        page,
                        at,
                        object: Object::Device(c),
                        which: t.to_string(),
                        fix: Some(Fix::Finalise),
                    });
                }
                if !doc.cables_tagged(t).is_empty() {
                    out.push(Check {
                        kind: Kind::DuplicateTag,
                        page,
                        at,
                        object: Object::Device(c),
                        which: format!("A cable and a device share {t}"),
                        fix: None,
                    });
                }
            }
        }
    }
    // Brief 025 (design 025 §4.3): drawn beyond the part's offer, ringed at the first such pole.
    for (which, page, at) in crate::xref::count_checks(doc, c) {
        out.push(Check { kind: Kind::TooManyContacts, page, at, object: Object::Device(c), which, fix: None });
    }
    // Brief 044 (design 044 §6.3): a connector's mating checks.
    out.extend(crate::connectors::checks(doc, c));
    // Brief 033 (design 033 §4.1): a table the user placed over the title block stays there.
    if let Some((which, page, at)) = crate::xref::title_check(doc, c, m) {
        out.push(Check {
            kind: Kind::TableOverTitle,
            page: Some(page),
            at: Some(at),
            object: Object::Device(c),
            which,
            fix: Some(Fix::MoveTable(c)),
        });
    }
    out
}

/// The rows of design 052 §6 for component `c`: a stored function or location naming no entry
/// (devices and boxes); for a device, a value none of its symbols' contexts has, and two heads.
/// O(its representations and their pages' boxes).
fn designation_checks(places: &mut Places<'_>, c: DeviceId, out: &mut Vec<Check>) {
    use crate::designations::{self as d, Aspect};
    let doc = places.doc();
    let Some(comp) = doc.devices.get(&c) else { return };
    let is_box = crate::locations::is_box_component(comp);
    if !is_box && !crate::locations::is_device(doc, c) {
        return;
    }
    let who = if is_box { "A box".to_string() } else { crate::devices::tag_words(doc, c) };
    let (page, at) = if is_box {
        doc.representations_of(Subject::Device(c)).next().map_or((None, None), |(_, r)| (Some(r.page), crate::locations::anchor(doc, r)))
    } else {
        crate::reps::goto_place_in(places, c).map_or((None, None), |(p, a)| (Some(p), a))
    };
    for (k, v) in &comp.properties {
        let Some(a) = Aspect::of_property(k) else { continue };
        if d::code_of(doc, a, v).is_none() {
            out.push(Check {
                kind: Kind::DesignationMissing,
                page,
                at,
                object: Object::Device(c),
                which: format!("{who}'s {} is no longer in the list", a.word()),
                fix: Some(Fix::SetDesignation(c, a)),
            });
        }
    }
    if is_box {
        return;
    }
    let reps: Vec<&ecad_model::Representation> = doc.representations_of(Subject::Device(c)).map(|(_, r)| r).collect();
    if reps.is_empty() {
        return;
    }
    let mut cache = crate::locations::BoxCache::default();
    for a in Aspect::ALL {
        let own = crate::locations::resolve_in(doc, c, a);
        let (Some(key), Some(code)) = (own.key.clone(), own.value.clone()) else { continue };
        let contexts: Vec<crate::locations::Located> = reps.iter().map(|r| crate::locations::rep_context_in(doc, a, r, &mut cache)).collect();
        if contexts.iter().any(|x| x.key.as_deref() == Some(key.as_str())) {
            continue;
        }
        // **Use +CP1** when every symbol is in one context that has a value.
        let shared = contexts.first().filter(|f| f.value.is_some() && contexts.iter().all(|x| x.key == f.key)).and_then(|f| f.value.clone());
        let use_words = shared.map(|s| format!(" (all of them are in {s})")).unwrap_or_default();
        out.push(Check {
            kind: if a == Aspect::Location { Kind::LocationUnused } else { Kind::FunctionUnused },
            page,
            at,
            object: Object::Device(c),
            which: format!("{who} is {code}, but none of its symbols is in {code}{use_words}"),
            fix: contexts.first().filter(|f| f.value.is_some() && contexts.iter().all(|x| x.key == f.key)).map(|_| Fix::UseContext(c, a)),
        });
    }
    let heads = crate::reps::heads(doc, c);
    if let [_, second, ..] = heads.as_slice()
        && let Some(r) = doc.representations.get(second)
    {
        let mut xp = crate::xref::Places::new(doc);
        let wheres: Vec<String> =
            heads.iter().filter_map(|h| doc.representations.get(h).map(|x| xp.text(x.page, crate::xref::anchor(doc, x)))).collect();
        out.push(Check {
            kind: Kind::TwoHeads,
            page: Some(r.page),
            at: crate::xref::anchor(doc, r),
            object: Object::Device(c),
            which: format!("{who} has two {}s ({})", crate::devices::head_noun(doc, *second), wheres.join(", ")),
            fix: None,
        });
    }
}

/// The rows of the project's list (design 052 §6): two entries of one aspect sharing a code.
pub fn designation_rows(doc: &Document) -> Vec<Check> {
    use crate::designations::{self as d, Aspect};
    let mut out = Vec::new();
    for a in Aspect::ALL {
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for e in d::entries(doc, a) {
            *seen.entry(e.code.to_uppercase()).or_default() += 1;
        }
        for (code, n) in seen {
            if n > 1 {
                out.push(Check {
                    kind: Kind::DesignationDuplicate,
                    page: None,
                    at: None,
                    object: Object::None,
                    which: format!("Two {} are called {code}", a.plural()),
                    fix: Some(Fix::ShowDesignations),
                });
            }
        }
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
    let which = format!("{} /?", crate::cables::cable_title(doc, k));
    vec![Check { kind: Kind::OrphanReference, page: Some(r.page), at, object: Object::Reference(*rid), which, fix: None }]
}

/// The checks of cable `k` (brief 026): a provisional tag; a tag another cable or a device holds.
pub fn cable_checks(doc: &Document, k: ecad_model::CableId) -> Vec<Check> {
    cable_checks_in(&mut Places::new(doc), k)
}

/// As [`cable_checks`], reading the book and grids through `places`.
pub fn cable_checks_in(places: &mut Places<'_>, k: ecad_model::CableId) -> Vec<Check> {
    let doc = places.doc();
    let mut out = Vec::new();
    // Brief 059 (design 059 §6): a cable not drawn yet has one Info row and no core rows; its
    // planned ends and glands are not checked until it is drawn.
    let drawn = crate::cables::is_drawn(doc, k);
    if !drawn && doc.cables.contains_key(&k) {
        let name = crate::cables::cable_title(doc, k);
        out.push(Check {
            kind: Kind::CableNotDrawn,
            page: None,
            at: None,
            object: Object::Cable(k),
            which: format!("{name} is not in the drawing yet."),
            fix: Some(Fix::AssignCable(k)),
        });
    }
    out.extend(crate::cables::value_checks(doc, k, drawn));
    // Brief 057 (owner rule, board 2.06): every core is connected at both ends or ticked Not used.
    let open = if drawn { crate::styles::cable::open_cores(doc, k) } else { Vec::new() };
    if !open.is_empty() {
        let (page, at) = ecad_model::numbering::rep_anchor(doc, Subject::Cable(k)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
        let name = crate::cables::cable_title(doc, k);
        for (i, o) in open.into_iter().enumerate() {
            // Brief 059 (design 059 §6): an unwired core is "not assigned".
            let which = if o.unwired {
                format!("{name} core {} is not assigned: slash or add a wire, or tick Not used.", o.label)
            } else {
                format!("{name} core {} is open at an end; connect it, or tick Not used", o.label)
            };
            out.push(Check {
                kind: Kind::CoreOpen,
                page,
                at,
                object: Object::Cable(k),
                which,
                fix: Some(Fix::CoreOpen(k, u16::try_from(i).unwrap_or(u16::MAX))),
            });
        }
    }
    // Tester re-check 2, item 4: `unused.` keys a file made elsewhere holds that ECAD does not read.
    let (bad, n) = crate::styles::cable::bad_unused(doc, k);
    if n > 0 {
        let (page, at) = ecad_model::numbering::rep_anchor(doc, Subject::Cable(k)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
        let name = crate::cables::cable_title(doc, k);
        let more = if n > bad.len() { format!(" (and {} more)", n - bad.len()) } else { String::new() };
        for (key, v) in &bad {
            out.push(Check {
                kind: Kind::RatingBadValue,
                page,
                at,
                object: Object::Cable(k),
                which: format!("{name} has {key} '{v}', which ECAD does not read{more}"),
                fix: None,
            });
        }
    }
    let Some(t) = crate::devices::cable_tag_of(doc, k) else { return out };
    if !ecad_model::tag::is_provisional(t) && doc.cables_tagged(t).len() < 2 && doc.components_tagged(t).is_empty() {
        return out;
    }
    let (page, at) = ecad_model::numbering::rep_anchor_in(places, Subject::Cable(k)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
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
    devices: BTreeSet<DeviceId>,
    /// Cables whose reference ends to re-check (brief 027: orphans).
    cable_references: BTreeSet<CableId>,
    /// Cables to re-check (brief 026: provisional and shared tags).
    cables: BTreeSet<ecad_model::CableId>,
    /// Brief 050a: the style rows (`crate::styles::checks::touches`).
    pub(crate) styles: bool,
    /// Brief 057: what the ops reach of the rating results.
    rating: crate::rating::Touch,
    /// Brief 052: the list's rows (an entry's code changed, added or removed).
    pub(crate) designations: bool,
    /// Brief 055: the title block rows (`crate::title_block::checks::touches`).
    pub(crate) title_blocks: bool,
}

impl Scope {
    /// Brief 054: whether the set of pages with checks off may change (then everything is checked
    /// again).
    fn checks_pages(&mut self, doc: &Document, ops: &[Op]) {
        if crate::page_types::checks_changed(doc, ops) {
            self.all = true;
        }
    }
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
        self.rating.collect(doc, ops);
        for op in ops {
            self.styles |= crate::styles::checks::touches(op);
            self.title_blocks |= crate::title_block::checks::touches(op);
            match op {
                Op::CreateConductor { id, conductor } | Op::DeleteConductor { id, conductor } => {
                    // Brief 057: a member's ends decide its cable's open cores.
                    self.cables.extend(doc.cables_of(*id));
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
                    self.cables.extend(doc.cables_of(*conductor));
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
                    self.devices.insert(terminal.component);
                    node(self, Node::Terminal(*id));
                }
                // Brief 048: another definition may bridge other points.
                Op::SetDeviceDefinition { id, .. } => {
                    self.devices.insert(*id);
                    for (t, _) in doc.terminals_of(*id) {
                        node(self, Node::Terminal(t));
                    }
                }
                Op::CreateDevice { id, device } | Op::DeleteDevice { id, device } => {
                    self.tag_holders(doc, device.properties.get(ecad_model::tag::TAG_KEY));
                    self.component(doc, *id, device.properties.get(NAME).map(String::as_str));
                    for (t, _) in doc.terminals_of(*id) {
                        node(self, Node::Terminal(t));
                    }
                }
                Op::ModifyDevice { id, before, after } => {
                    for device in [before, after] {
                        self.tag_holders(doc, device.properties.get(ecad_model::tag::TAG_KEY));
                        self.component(doc, *id, device.properties.get(NAME).map(String::as_str));
                    }
                    for (t, _) in doc.terminals_of(*id) {
                        node(self, Node::Terminal(t));
                    }
                }
                Op::SetProperty { object: ObjectRef::Device(c), key, before, after } => {
                    // Brief 052 (design 052 §6): a box's value is a context for the devices on its
                    // pages.
                    if crate::locations::is_box(doc, *c) {
                        for p in doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page).collect::<Vec<_>>() {
                            self.devices_on(doc, p);
                        }
                    }
                    if key == ecad_model::tag::TAG_KEY {
                        self.tag_holders(doc, [before, after].into_iter().flatten());
                    }
                    // Brief 059 (design 059 §8): a device's location or function is where the
                    // cables at its points are drawn (`cable-end-off-plan`).
                    if crate::designations::Aspect::of_property(key).is_some() {
                        for (t, _) in doc.terminals_of(*c) {
                            for (w, _) in doc.ends_at(t) {
                                self.cables.extend(doc.cables_of(w));
                            }
                        }
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
                        self.devices.insert(c);
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
                        self.devices.extend(crate::xref::placed_tables(doc));
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
                        if let Subject::Device(c) = r.subject {
                            self.devices.insert(c);
                        }
                    }
                }
                // Brief 052 (design 052 §6): a page's function or location is a context for the
                // devices on it; the project's for every device (rare); the list's codes for the
                // objects storing that entry, and the duplicate rows.
                Op::SetProperty { object: ObjectRef::Page(p), key, .. } if crate::designations::Aspect::of_property(key).is_some() => {
                    self.relocate = true;
                    self.devices_on(doc, *p);
                }
                Op::SetProperty { object: ObjectRef::Project, key, .. } if crate::designations::Aspect::of_property(key).is_some() => {
                    self.relocate = true;
                    self.devices.extend(doc.devices.keys().copied());
                }
                Op::SetProperty { object: ObjectRef::Project, key, .. } if crate::designations::is_list_property(key) => {
                    self.relocate = true;
                    self.designations = true;
                    if let Some((_, entry, true)) = crate::designations::parse_list_property(key) {
                        // Brief 059: the cables planning an end there (their rows name its code).
                        self.cables.extend(crate::cables::planning(doc, entry));
                        for (c, x) in &doc.devices {
                            if x.properties.iter().any(|(k, v)| crate::designations::Aspect::of_property(k).is_some() && v == entry) {
                                self.devices.insert(*c);
                            }
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
                // Brief 059 (design 059 §8): a cable type's cores and gland reach its users' rows.
                Op::SetStyle { catalogue, id, .. } if catalogue == crate::styles::cable::WORD => {
                    self.cables.extend(crate::styles::cable::users(doc, id));
                }
                // Brief 050a: style records and the standard reach only the style rows (above).
                Op::SetStyle { .. } | Op::SetStandard { .. } => {}
            }
        }
    }

    /// Every device with a symbol on `page` (brief 052: their contexts may have changed).
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
    fn tag_holders<'t>(&mut self, doc: &Document, tags: impl IntoIterator<Item = &'t String>) {
        for t in tags {
            self.devices.extend(doc.components_tagged(t));
            self.cables.extend(doc.cables_tagged(t));
        }
    }

    fn component_of(&mut self, doc: &Document, t: TerminalId) {
        if let Some(c) = doc.terminals.get(&t).map(|x| x.component) {
            self.devices.insert(c);
        }
    }

    /// A component and, for a continuation name, every continuation with that name.
    fn component(&mut self, doc: &Document, c: DeviceId, name: Option<&str>) {
        self.devices.insert(c);
        let name = name.map(str::to_string).or_else(|| doc.devices.get(&c).and_then(|x| x.properties.get(NAME).cloned()));
        if let Some(n) = name {
            self.devices.extend(doc.continuations_named(&name_key(&n)));
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
            Subject::Device(c) => {
                // Brief 052: a box drawn, moved or erased changes the contexts on its page.
                if crate::locations::is_box(doc, c) {
                    self.devices_on(doc, r.page);
                }
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

/// `rows` in book order.
pub fn sort(doc: &Document, mut rows: Vec<Check>) -> Vec<Check> {
    let order: HashMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    rows.sort_by_key(|c| c.order_key(&order));
    rows
}

/// Every check of the project with the report models already kept, measuring text with `m`
/// (ecad-cadcraft's `crate::checks::all` and `crate::checks::all_with` pass CADCraft's fonts).
pub fn all_measured(doc: &Document, tol: f64, outputs: &crate::outputs::kept::Outputs, m: &dyn TextMetrics) -> Vec<Check> {
    let mut rows: Vec<Check> = kept::Checks::build_with(doc, tol, m).rows().cloned().collect();
    rows.extend(outputs.checks());
    sort(doc, rows)
}

// The engine half (design 065 §3), at its old path until step 6 turns this into a facade.
