//! The kept report models (see [`super`]'s update map). The kept PLC model's rules measure text
//! (an address wider than its cell), so building and updating take the text look,
//! [`Look`]; the project keeps them in its `derived::ReportsView`, which passes the
//! engine's look.
//!
//! Split by job: this file holds the models' state and what an effect notes; `update` re-computes rows,
//! `checks` answers the checks, `rows` reads the BOM, wire and cable rows, `tables` makes the report tables.

use ecad_model::document::effect::Effect;
use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{CableId, ConnectionPointId, DeviceId, Document, End, LoosePartId, ObjectRef, PageId, Pt, Subject, WireId};

mod checks;
mod rows;
mod tables;
mod update;

use crate::reports::*;
use ecad_model::look::Look;

/// A column every cable report has, before the optional [`CableColumn`]s: by what it shows, so nothing reads a header back from text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CableBase {
    Tag,
    Type,
    Cores,
    FromLocation,
    ToLocation,
    Used,
    Spare,
    From,
    To,
}

impl CableBase {
    /// Every column, in the order the table shows them.
    pub const ALL: [CableBase; 9] = [
        CableBase::Tag,
        CableBase::Type,
        CableBase::Cores,
        CableBase::FromLocation,
        CableBase::ToLocation,
        CableBase::Used,
        CableBase::Spare,
        CableBase::From,
        CableBase::To,
    ];

    /// The column's header.
    pub fn header(self) -> &'static str {
        match self {
            CableBase::Tag => "Tag",
            CableBase::Type => "Type",
            CableBase::Cores => "Cores",
            CableBase::FromLocation => "From location",
            CableBase::ToLocation => "To location",
            CableBase::Used => "Used",
            CableBase::Spare => "Spare",
            CableBase::From => "From",
            CableBase::To => "To",
        }
    }

    /// The headers of every column, in order.
    pub fn headers() -> Vec<&'static str> {
        Self::ALL.iter().map(|c| c.header()).collect()
    }
}

/// A column of the wire report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireColumn {
    WireNumber,
    From,
    FromLocation,
    To,
    ToLocation,
    CrossSection,
    Colour,
    Potential,
    Category,
    Cable,
    Page,
}

impl WireColumn {
    /// Every column, in the order the table shows them.
    pub const ALL: [WireColumn; 11] = [
        WireColumn::WireNumber,
        WireColumn::From,
        WireColumn::FromLocation,
        WireColumn::To,
        WireColumn::ToLocation,
        WireColumn::CrossSection,
        WireColumn::Colour,
        WireColumn::Potential,
        WireColumn::Category,
        WireColumn::Cable,
        WireColumn::Page,
    ];

    /// The column's header.
    pub fn header(self) -> &'static str {
        match self {
            WireColumn::WireNumber => "Wire number",
            WireColumn::From => "From",
            WireColumn::FromLocation => "From location",
            WireColumn::To => "To",
            WireColumn::ToLocation => "To location",
            WireColumn::CrossSection => "Cross-section",
            WireColumn::Colour => "Colour",
            WireColumn::Potential => "Potential",
            WireColumn::Category => "Category",
            WireColumn::Cable => "Cable",
            WireColumn::Page => "Page",
        }
    }

    /// The headers of every column, in order.
    pub fn headers() -> Vec<&'static str> {
        Self::ALL.iter().map(|c| c.header()).collect()
    }
}

/// A column of a cable's core table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreColumn {
    Core,
    WireNumber,
    From,
    To,
    State,
}

impl CoreColumn {
    /// Every column, in the order the table shows them.
    pub const ALL: [CoreColumn; 5] = [CoreColumn::Core, CoreColumn::WireNumber, CoreColumn::From, CoreColumn::To, CoreColumn::State];

    /// The column's header.
    pub fn header(self) -> &'static str {
        match self {
            CoreColumn::Core => "Core",
            CoreColumn::WireNumber => "Wire number",
            CoreColumn::From => "From",
            CoreColumn::To => "To",
            CoreColumn::State => "State",
        }
    }

    /// The headers of every column, in order.
    pub fn headers() -> Vec<&'static str> {
        Self::ALL.iter().map(|c| c.header()).collect()
    }
}

/// An optional column of the cable report ([`ReportModels::cable_columns`]), by what it shows:
/// its header and its JSON key come from here, so nothing reads one back from the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CableColumn {
    Maker,
    Length,
    Designation,
    Glands,
    Shield,
    PlannedFrom,
    PlannedTo,
    Drawn,
}

impl CableColumn {
    /// Every optional column, in the order the report shows them.
    pub const ALL: [CableColumn; 8] = [
        CableColumn::Maker,
        CableColumn::Length,
        CableColumn::Designation,
        CableColumn::Glands,
        CableColumn::Shield,
        CableColumn::PlannedFrom,
        CableColumn::PlannedTo,
        CableColumn::Drawn,
    ];

    /// The column's header.
    pub fn header(self) -> &'static str {
        match self {
            CableColumn::Maker => "Maker",
            CableColumn::Length => "Length",
            CableColumn::Designation => "Designation",
            CableColumn::Glands => "Glands",
            CableColumn::Shield => "Shield",
            CableColumn::PlannedFrom => "Planned from",
            CableColumn::PlannedTo => "Planned to",
            CableColumn::Drawn => "Drawn",
        }
    }

    /// The column's key in the JSON rows of `cablelist`.
    pub fn key(self) -> &'static str {
        match self {
            CableColumn::Maker => "maker",
            CableColumn::Length => "length",
            CableColumn::Designation => "designation",
            CableColumn::Glands => "glands",
            CableColumn::Shield => "shield",
            CableColumn::PlannedFrom => "planned_from",
            CableColumn::PlannedTo => "planned_to",
            CableColumn::Drawn => "drawn",
        }
    }

    /// Whether any of `rows` has something to show in this column.
    fn shown(self, rows: &[&CableRow]) -> bool {
        match self {
            CableColumn::Maker => rows.iter().any(|r| r.maker.is_some()),
            CableColumn::Length => rows.iter().any(|r| r.length.is_some()),
            CableColumn::Designation => rows.iter().any(|r| r.designation.is_some()),
            CableColumn::Glands => rows.iter().any(|r| r.glands.iter().any(Option::is_some)),
            CableColumn::Shield => rows.iter().any(|r| r.shield.is_some()),
            CableColumn::PlannedFrom | CableColumn::PlannedTo => rows.iter().any(|r| r.planned.iter().any(Option::is_some)),
            CableColumn::Drawn => rows.iter().any(|r| !r.drawn),
        }
    }

    /// The cell of cable row `r`.
    fn cell(self, r: &CableRow) -> Cell {
        let dash = |v: &Option<String>| v.clone().unwrap_or_else(|| "\u{2014}".into());
        let planned = |i: usize| r.planned[i].as_ref().map(|p| format!("{p} (planned)")).unwrap_or_default();
        match self {
            CableColumn::Maker => Cell::ok(r.maker.clone().unwrap_or_default()),
            CableColumn::Length => Cell::ok(r.length.clone().unwrap_or_default()),
            CableColumn::Designation => Cell::ok(r.designation.clone().unwrap_or_default()),
            CableColumn::Glands => Cell::ok(format!("{}, {}", dash(&r.glands[0]), dash(&r.glands[1]))),
            CableColumn::Shield => Cell::ok(r.shield.clone().unwrap_or_default()),
            CableColumn::PlannedFrom => Cell::ok(planned(0)),
            CableColumn::PlannedTo => Cell::ok(planned(1)),
            CableColumn::Drawn => Cell::ok(if r.drawn { "yes" } else { "no" }),
        }
    }
}

/// The kept report models (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct ReportModels {
    devices: BTreeMap<DeviceId, DeviceRow>,
    /// Items whose location is the project's, or none: a project location set reaches
    /// them. Devices never are (a device's location is stored).
    from_project: BTreeSet<LoosePartId>,
    from_none: BTreeSet<LoosePartId>,
    /// The BOM entries (main parts, part lines, items) and the
    /// rows they make, kept by key.
    entries: BTreeMap<(Owner, u8), Entry>,
    by_location: BTreeMap<(LocKey, PartKey), BTreeSet<EntrySort>>,
    by_part: BTreeMap<PartKey, BTreeSet<EntrySort>>,
    /// The nested layout's rows: main parts and items only (a device with part lines a row of its
    /// own), and the part lines per location, so its sheet count is O(locations) too.
    nested: BTreeMap<(LocKey, PartKey), BTreeSet<EntrySort>>,
    loc_nested_rows: BTreeMap<LocKey, usize>,
    loc_lines: BTreeMap<LocKey, usize>,
    /// Part key → its article numbers with how many entries carry each.
    articles: BTreeMap<PartKey, BTreeMap<String, usize>>,
    /// Part keys with two or more article numbers.
    article_conflicts: BTreeSet<PartKey>,
    /// Lines whose `for` names sub-items the device lacks.
    for_missing: BTreeSet<(DeviceId, u8)>,
    /// `part.` keys ECAD does not read, per component.
    bad_part_keys: BTreeMap<DeviceId, Vec<String>>,
    /// Uncovered fuses per device or strip, and lines that are fuse inserts (any `for`, or a line
    /// on a fuse terminal device), per component.
    fuse_gaps: BTreeMap<DeviceId, usize>,
    inserts: BTreeMap<DeviceId, usize>,
    lines_no_part: usize,
    items_no_part: usize,
    /// The report settings the tables use (`bom.column.article`, `bom.lines`).
    pub view: BomView,
    wires: BTreeMap<WireId, WireRow>,
    /// Conductors by their net's potential (lower case; `""`: none), for wiring-table changes.
    by_potential: BTreeMap<String, BTreeSet<WireId>>,
    cables: BTreeMap<CableId, CableRow>,
    /// Rows of the BOM-by-location table per location (a heading each), for its sheet count.
    loc_rows: BTreeMap<LocKey, usize>,
    /// The styles the tables write codes and places in.
    style: ecad_model::ReferenceStyle,
    colour_style: ecad_model::geometry::colour::ColourWords,
    /// Location box checks by page.
    box_checks: BTreeMap<PageId, Vec<Check>>,
    no_part_number: usize,
    no_location: usize,
    /// Devices with no function (one count row when some device has one).
    no_function: usize,
    /// The terminal strips.
    pub strips: crate::terminal_strips::strip_model::Strips,
    /// The PLC racks and cards.
    pub plc: crate::plc::kept::Plc,
    /// The graphic strip pages' kept layout and indexes.
    pub strip_pages: crate::terminal_strips::pages::StripPages,
    /// Whether the last update changed a cable row (what a shown cable schedule page
    /// depends on).
    pub last_cables_changed: bool,
    /// The pages drawn from these models (BOM, terminal plan, cable schedule), kept from the page
    /// ops (no page scan per commit).
    generated: BTreeSet<PageId>,
    /// Which build these models are (unique in the run): a copy keyed on kept revisions also
    /// keys on this, so a rebuild (reload, branch switch) is never mistaken for the old models.
    pub build_id: u64,
    /// How many sorted row lists were made (tests: an unrelated edit makes none).
    pub sorts: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// The location box rows by severity (warnings, info): kept with the rows (the badge).
    box_counts: [usize; 2],
    /// Bumped only when the check rows may read differently (the Release dialog).
    pub checks_changed: u64,
    /// Cable Checks rows, kept: `too-many-cores` warnings, `core-twice` warnings, and
    /// cables with no core count (one info row when any).
    cable_checks: [usize; 3],
    /// Each connector's BOM rows, one per drawn half.
    connectors: BTreeMap<DeviceId, Vec<BomRow>>,
    /// The half rows' groups, kept beside `loc_rows` / `by_part`:
    /// (location, group) and group → how many halves; location → how many groups.
    half_loc: BTreeMap<(LocKey, HalfKey), usize>,
    half_part: BTreeMap<HalfKey, usize>,
    half_loc_rows: BTreeMap<LocKey, usize>,
    /// How many times the half rows were grouped (tests: an unrelated edit groups none).
    pub half_groupings: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Devices that are not strips but carry `jumper_bar.<n>` keys.
    jumper_bars_off_strip: BTreeSet<DeviceId>,
    jumper_bar_keys: BTreeMap<DeviceId, Vec<String>>,
    /// The `continuation-clash` rows, kept. Worked out again only
    /// when a page is added, removed or renumbered, an issue changes the revision list, or a kept
    /// generated page's sheet count moves.
    continuation: Vec<Check>,
    /// How many times `Self::continuation` was worked out (tests: an unrelated edit makes none).
    pub continuation_builds: u64,
    /// Bumped on every change, for views that copy the rows.
    pub revision: u64,
    /// How many rows the last update recomputed (tests and bench: never the whole project).
    pub last_work: usize,
    /// The last update's ops could move a page number or a sheet count that
    /// the document alone decides (a page's style or sheet): the frame's Total is worked out again.
    pub numbers_moved: bool,
}

/// What one commit's effects name for the report models, noted effect by effect
/// ([`ReportModels::note_before`], [`ReportModels::note_after`]) and recomputed by [`ReportModels::update`].
#[derive(Debug, Clone, Default)]
pub(crate) struct Noted {
    /// The rows: noted on the state before and on the state after.
    rows: Scope,
    /// Wires named only on the state after, added after the net mates ([`Scope::note_after`]).
    late: Scope,
    /// The PLC model's touch.
    plc: crate::plc::kept::Touch,
    /// What the strip pages' indexes are read for.
    strips: crate::terminal_strips::pages::Named,
    /// A page number or a sheet count can move ([`ReportModels::moves_page_number`]).
    numbers: bool,
    /// Pages added, removed or moved ([`ReportModels::placed_page`]).
    placed: Vec<PageId>,
    /// The pages whose book position changes (the strips on them): the book before, compared
    /// with the state after.
    positions: ecad_model::document::book::PositionsNoted,
}

/// [`Noted`] without the strip pages' names, for the row update.
struct Rows {
    scope: Scope,
    late: Scope,
    plc: crate::plc::kept::Touch,
    placed: Vec<PageId>,
    positions: ecad_model::document::book::PositionsNoted,
}

impl ReportModels {
    /// Every model, from scratch (open, reload, first show), with text measured in `m`.
    pub fn build_with(doc: &Document, m: &dyn Look) -> ReportModels {
        let mut o = ReportModels {
            style: doc.settings().reference_style(),
            colour_style: doc.settings().wiring.colour_style(),
            view: BomView::of(doc),
            ..ReportModels::default()
        };
        let mut cache = BoxCache::default();
        for (_, c) in doc.tagged() {
            o.set_device(doc, c, &mut cache);
        }
        // Items (loose parts, on no page).
        for id in doc.loose_parts().keys() {
            o.set_loose(doc, *id);
        }
        let cx = PageCx::of(doc);
        for w in doc.wires().keys() {
            o.set_wire(doc, *w, &cx);
        }
        for k in doc.cables().keys() {
            o.set_cable(doc, *k);
        }
        for p in doc.page_order() {
            o.set_box_checks(doc, *p);
        }
        o.strips = crate::terminal_strips::strip_model::Strips::build(doc, &o, &cx);
        o.plc = crate::plc::kept::Plc::build_with(doc, m);
        o.strip_pages = crate::terminal_strips::pages::StripPages::build(doc, &o.strips);
        o.generated = doc.page_order().iter().copied().filter(|p| needs_reports(doc, *p)).collect();
        static BUILDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        o.build_id = BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        o.refresh_continuation(doc);
        o
    }

    /// Work out the `continuation-clash` rows.
    fn refresh_continuation(&mut self, doc: &Document) {
        let rows = crate::pages::generated::continuation_checks(doc, Some(&*self));
        self.continuation_builds += 1;
        if rows != self.continuation {
            self.continuation = rows;
            self.checks_changed += 1;
        }
    }

    /// The sheet count of each kept generated page (BOM, terminal plan, cable schedule).
    fn generated_sheets(&self, doc: &Document) -> Vec<(PageId, usize)> {
        self.generated.iter().map(|p| (*p, crate::pages::generated::sheets_of(doc, *p, Some(self)))).collect()
    }

    /// Whether `effect` can move a page number or a contents or revision list sheet count: a page
    /// added, removed or renumbered, a page's kind, or an issue.
    pub fn moves_page_number(effect: &ecad_model::document::effect::Effect<'_>) -> bool {
        use ecad_model::document::effect::Effect;
        let style = crate::defaults::strip_pages::STYLE.key;
        match effect {
            // A page's style or sheet changes its sheet count on the document
            // alone, so the count before cannot be compared.
            Effect::Page { before: Some(before), after: Some(after), index_before: None, index_after: None, .. } => {
                before.number != after.number || before.sheet != after.sheet || before.display.get(style) != after.display.get(style)
            }
            // Added, removed, or both (a move).
            Effect::Page { .. } => true,
            Effect::Prop { object: ObjectRef::Page(_) | ObjectRef::Project | ObjectRef::Standard, key, .. } => key.raw == style,
            Effect::Project { before, after } => crate::releases::revisions::issue_changed(before, after),
            Effect::Settings { before, after } => before.sheet != after.sheet,
            Effect::Prop { .. }
            | Effect::Layer { .. }
            | Effect::Item { .. }
            | Effect::Device { .. }
            | Effect::DeviceDefinition { .. }
            | Effect::ConnectionPoint { .. }
            | Effect::Rep { .. }
            | Effect::Wire { .. }
            | Effect::Route { .. }
            | Effect::Cable { .. }
            | Effect::Definition { .. }
            | Effect::Version { .. }
            | Effect::Folder { .. }
            | Effect::Style { .. }
            | Effect::Standard { .. }
            | Effect::Aid { .. }
            | Effect::LoosePart { .. }
            | Effect::DeviceLink { .. } => false,
        }
    }

    /// The page `effect` adds, removes or moves (a page changed in place states no index): whether
    /// it is drawn from these models is read again on the state after.
    pub fn placed_page(effect: &ecad_model::document::effect::Effect<'_>) -> Option<PageId> {
        use ecad_model::document::effect::Effect;
        match effect {
            Effect::Page { id, index_before, index_after, .. } if index_before.is_some() || index_after.is_some() => Some(*id),
            Effect::Page { .. }
            | Effect::Layer { .. }
            | Effect::Item { .. }
            | Effect::Device { .. }
            | Effect::DeviceDefinition { .. }
            | Effect::ConnectionPoint { .. }
            | Effect::Rep { .. }
            | Effect::Wire { .. }
            | Effect::Route { .. }
            | Effect::Cable { .. }
            | Effect::Prop { .. }
            | Effect::Definition { .. }
            | Effect::Settings { .. }
            | Effect::Version { .. }
            | Effect::Folder { .. }
            | Effect::Style { .. }
            | Effect::Standard { .. }
            | Effect::Project { .. }
            | Effect::Aid { .. }
            | Effect::LoosePart { .. }
            | Effect::DeviceLink { .. } => None,
        }
    }

    /// The pages drawn from these models.
    pub fn generated_pages(&self) -> &BTreeSet<PageId> {
        &self.generated
    }

    /// How many BOM rows `group` lists, from kept counts (no rows made).
    pub fn bom_count(&self, group: Group) -> usize {
        let strips: usize = self.strips.bom_rows.values().sum();
        // Grouped as `bom` groups them (O(connectors)).
        let halves = match group {
            Group::Location => self.half_loc.len(),
            Group::Part => self.half_part.len(),
        };
        match group {
            Group::Location if self.view.nested => self.nested.len() + self.loc_lines.values().sum::<usize>() + strips + halves,
            Group::Location => self.by_location.len() + strips + halves,
            Group::Part => self.by_part.len() + strips + halves,
        }
    }

    /// How many wire rows.
    pub fn wire_count(&self) -> usize {
        self.wires.len()
    }

    /// One cable's row.
    pub fn cable_row(&self, k: CableId) -> Option<&CableRow> {
        self.cables.get(&k)
    }

    /// How many cables (no sort).
    pub fn cable_count(&self) -> usize {
        self.cables.len()
    }

    /// The strips sorted by tag (counted in [`ReportModels::sorts`]).
    pub fn sorted_strips(&self) -> Vec<&crate::terminal_strips::strip_model::StripData> {
        self.sorts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.strips.sorted()
    }

    /// Before phase: note what one effect, about to be committed on `before`, names. Pair with
    /// [`ReportModels::close_before`], [`ReportModels::note_after`] and [`ReportModels::update`].
    pub(crate) fn note_before(&self, before: &Document, effect: &Effect<'_>, n: &mut Noted) {
        n.rows.note(before, effect);
        n.positions.note_before(before, effect);
    }

    /// The end of the Before phase: the noted wires' net mates and the users of renamed entries on
    /// `before`.
    pub(crate) fn close_before(&self, before: &Document, n: &mut Noted) {
        n.rows.close(before);
    }

    /// After phase: note what one effect of the transaction just committed names on `after`,
    /// reading the models as they are before the update.
    pub(crate) fn note_after(&self, after: &Document, effect: &Effect<'_>, n: &mut Noted) {
        n.rows.note(after, effect);
        n.late.note_after(after, effect);
        self.plc.note(after, effect, &mut n.plc);
        n.strips.note(effect);
        n.numbers |= Self::moves_page_number(effect);
        n.placed.extend(Self::placed_page(effect));
    }
}
