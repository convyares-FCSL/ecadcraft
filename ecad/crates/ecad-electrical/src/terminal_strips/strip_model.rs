//! The strip model: a strip's rows and accessories as derived from the drawing,
//! and [`Strips`], the set kept with the report models and its update map.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::connectivity::derive::Place;
use ecad_model::{CableId, ConnectionPointId, DeviceId, Document, End, PageId, Pt, RepresentationId, WireId};

use super::*;
use crate::reports::PageCx;
use crate::reports::kept::ReportModels;

/// One conductor on one side of a terminal.
#[derive(Debug, Clone, PartialEq)]
pub struct SideCell {
    pub conductor: WireId,
    /// The far end as shown (`-K01:A1`, `24V >`); `None` for a loose end.
    pub target: Option<String>,
    pub wire: Option<String>,
    pub cable: Option<(CableId, Option<String>)>,
    /// The member's stored core label.
    pub core: Option<String>,
    /// The far end's component (a device or a continuation).
    pub far: Option<DeviceId>,
}

/// One row of a strip's plan: one level of a position (a one-level position is
/// one row).
#[derive(Debug, Clone, PartialEq)]
pub struct TermRow {
    /// The level's terminal (the in point of a two-point level); `None` for a free or spare row.
    pub connection_point: Option<ConnectionPointId>,
    /// The out point of a two-point level (its outside cells come from it).
    pub out: Option<ConnectionPointId>,
    /// The **position** label (what jumper bars, accessories and renumbering name).
    pub label: String,
    /// The shown label: `1.L`, or the level's own label.
    pub shown: String,
    pub label_overridden: bool,
    /// The level number (1 = top deck) and its name.
    pub level: u32,
    pub level_name: String,
    /// The position's type.
    pub ty: &'static crate::terminal_strips::terminal_blocks::TermType,
    /// A level of a drawn position that no symbol draws.
    pub free: bool,
    pub pe: bool,
    /// A fuse position's rating (`rating.<p>`).
    pub rating: Option<String>,
    /// The internal bridge group of the position's type this level is in (1…).
    pub internal_bridge: Option<usize>,
    pub inside: Vec<SideCell>,
    pub outside: Vec<SideCell>,
    /// The bridge group (1, 2… per strip) this terminal is in.
    pub bridge_group: Option<usize>,
    pub place: Option<Place>,
    pub go: Option<(PageId, Pt)>,
}

impl TermRow {
    /// A position the offer adds, with no level drawn.
    pub fn spare(&self) -> bool {
        self.connection_point.is_none() && !self.free
    }

    /// `drawn`, `free` or `spare`.
    pub fn state(&self) -> &'static str {
        if self.free {
            "free"
        } else if self.connection_point.is_none() {
            "spare"
        } else {
            "drawn"
        }
    }

    /// Whether the position's type breaks the net (disconnect, fuse).
    pub fn breaks(&self) -> bool {
        self.ty.breaks
    }

    fn blank(label: String, ty: &'static crate::terminal_strips::terminal_blocks::TermType, level: u32, level_name: String) -> TermRow {
        TermRow {
            connection_point: None,
            out: None,
            shown: label.clone(),
            label,
            label_overridden: false,
            level,
            level_name,
            ty,
            free: false,
            pe: ty.pe_level == Some(level),
            rating: None,
            internal_bridge: None,
            inside: Vec::new(),
            outside: Vec::new(),
            bridge_group: None,
            place: None,
            go: None,
        }
    }
}

/// One terminal type on a strip: positions drawn and spare, and its BOM part data.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeCount {
    pub ty: &'static crate::terminal_strips::terminal_blocks::TermType,
    pub drawn: usize,
    pub spare: usize,
    pub part_number: Option<String>,
    pub maker: Option<String>,
    /// Read at the level its part number came from.
    pub article_number: Option<String>,
    pub description: String,
}

impl TypeCount {
    /// BOM quantity: positions, drawn and spare.
    pub fn qty(&self) -> usize {
        self.drawn + self.spare
    }
}

/// One strip's derived data.
#[derive(Debug, Clone, PartialEq)]
pub struct StripData {
    pub component: DeviceId,
    pub tag: String,
    pub location: Option<String>,
    pub offer: Option<Offer>,
    /// The `connection_points` text when it could not be read (shown, not used).
    pub drawn: usize,
    pub drawn_pe: usize,
    pub spare: usize,
    pub spare_pe: usize,
    pub rows: Vec<TermRow>,
    pub part_number: Option<String>,
    pub maker: Option<String>,
    pub part_number_pe: Option<String>,
    pub maker_pe: Option<String>,
    pub description: Option<String>,
    /// Terminals drawn with a label an earlier one has: (label, the second terminal); a level
    /// drawn twice (`level-twice`).
    pub twice: Vec<(String, ConnectionPointId)>,
    /// Positions and their BOM part data per type, in catalogue order (types with
    /// positions only).
    pub types: Vec<TypeCount>,
    /// Positions whose levels are drawn by symbols of another type: (position, a terminal).
    pub mixed: Vec<(String, ConnectionPointId)>,
    /// Jumper bars naming levels their positions lack: (jumper bar number, the items as written).
    pub level_missing: Vec<(u32, Vec<String>)>,
    /// Fuse positions with no rating.
    pub no_rating: usize,
    /// Each row's position index.
    pos_of_row: Vec<usize>,
    /// The type word of each `mixed` terminal's own symbol.
    mixed_types: BTreeMap<ConnectionPointId, &'static str>,
    /// Sheets of its terminal plan section and rows of its BOM:
    /// kept with the strip, so sheet counts never walk the rows.
    pub sheets: usize,
    pub bom_rows: usize,
    /// The accessories placed and the jumper bars.
    pub acc: crate::terminal_strips::accessories::StripAcc,
    /// What this strip's rows read, for the update map.
    wires: BTreeSet<WireId>,
    far: BTreeSet<DeviceId>,
    pages: BTreeSet<PageId>,
    cables: BTreeSet<CableId>,
}

impl StripData {
    /// `12 positions: 10 drawn, 2 spare`, or `4 drawn` with no offer.
    pub fn counts_text(&self) -> String {
        match self.offer {
            Some(o) => format!("{} positions: {} drawn, {} spare", o.positions, self.drawn, self.spare),
            None => format!("{} drawn", self.drawn),
        }
    }

    /// Feed-through and PE quantities for the BOM: drawn plus spare of each type.
    pub fn quantities(&self) -> (usize, usize) {
        let q = |code: &str| self.types.iter().find(|t| t.ty.code == code).map_or(0, TypeCount::qty);
        (q("ft"), q("pe"))
    }

    /// The types whose drawn positions are more than the offer gives them.
    pub fn over_offer(&self) -> Vec<(&TypeCount, u32)> {
        let Some(o) = self.offer else { return Vec::new() };
        // With no type codes: feed-through and PE counted together.
        if o.counts.iter().skip(2).all(|n| *n == 0) && self.types.iter().all(|t| t.ty.legacy()) {
            return match self.types.first() {
                Some(t) if self.drawn > o.positions as usize => vec![(t, o.positions)],
                _ => Vec::new(),
            };
        }
        self.types.iter().filter(|t| t.drawn > o.of(t.ty) as usize).map(|t| (t, o.of(t.ty))).collect()
    }

    /// Its Checks rows: (`terminal-block-twice` errors, `too-many-terminal-blocks` and `position-mixed`
    /// warnings).
    pub fn check_rows(&self) -> (usize, usize) {
        (self.twice.len(), self.over_offer().len() + self.mixed.len() + self.level_missing.len() + self.acc.warnings())
    }

    /// The highest drawn terminal (by label), for the too-many-terminals go-to.
    pub fn highest(&self) -> Option<&TermRow> {
        self.rows.iter().rev().find(|r| r.connection_point.is_some())
    }

    /// The highest drawn row of type `t`.
    pub fn highest_of(&self, t: &crate::terminal_strips::terminal_blocks::TermType) -> Option<&TermRow> {
        self.rows.iter().rev().find(|r| r.connection_point.is_some() && r.ty == t)
    }

    /// The plan index of row `i`'s position (rows of one position share it).
    pub fn position_index(&self, i: usize) -> usize {
        self.pos_of_row.get(i).copied().unwrap_or(0)
    }

    /// The type word of the symbol that draws terminal `t` (`position-mixed`).
    pub fn other_type_word(&self, t: ConnectionPointId) -> &'static str {
        self.mixed_types.get(&t).copied().unwrap_or("?")
    }

    /// Whether row `i` is the first row of its position.
    pub fn first_of_position(&self, i: usize) -> bool {
        i == 0 || self.rows.get(i - 1).zip(self.rows.get(i)).is_some_and(|(a, b)| a.label != b.label)
    }

    /// A drawn row by terminal.
    pub fn row_of(&self, t: ConnectionPointId) -> Option<&TermRow> {
        self.rows.iter().find(|r| r.connection_point == Some(t))
    }

    /// The plan in physical order: `acc.start`'s items, each position followed
    /// by its `acc.after` items, then `acc.end`'s items. With `accessories` off, the plain rows only.
    pub fn plan(&self, accessories: bool) -> Vec<PlanItem<'_>> {
        let mut out = Vec::with_capacity(self.rows.len() + self.acc.gap_rows());
        fn gap<'a>(out: &mut Vec<PlanItem<'a>>, on: bool, g: &'a str, items: &[crate::terminal_strips::accessories::Acc]) {
            if on {
                out.extend(items.iter().map(|a| PlanItem::Gap { gap: g, acc: *a }));
            }
        }
        gap(&mut out, accessories, "start", &self.acc.start);
        for (i, r) in self.rows.iter().enumerate() {
            out.push(PlanItem::Term(i, r));
            let last_of_label = self.rows.get(i + 1).is_none_or(|n| n.label != r.label);
            if last_of_label && let Some(items) = self.acc.after.get(&r.label) {
                gap(&mut out, accessories, &r.label, items);
            }
        }
        gap(&mut out, accessories, "end", &self.acc.end);
        out
    }

    /// The jumper bars through plan row `i`: (jumper bar, whether its level of its position is one the jumper bar
    /// names). A jumper bar runs on its own level; a cross-level jumper bar only through the
    /// levels of its position.
    pub fn jumper_bars_at(&self, i: usize) -> Vec<(&crate::terminal_strips::accessories::JumperBarInfo, bool)> {
        let Some(r) = self.rows.get(i) else { return Vec::new() };
        let pi = self.position_index(i);
        self.acc
            .jumper_bars
            .iter()
            .filter(|l| l.span > 0 && l.first <= pi && pi <= l.last && l.runs_on(r.level))
            .map(|l| (l, l.labels.contains(&r.label) && l.names_level(r.level)))
            .collect()
    }

    /// The Bridge cell of a terminal row: `● n`, then one mark per jumper bar
    /// lane: `●` on a linked position, `┆` past a skipped one.
    pub fn bridge_cell(&self, i: usize) -> String {
        let mut text = self.rows.get(i).and_then(|r| r.bridge_group).map_or_else(String::new, |g| format!("\u{25cf} {g}"));
        let at = self.jumper_bars_at(i);
        let lanes = at.iter().map(|(l, _)| l.lane + 1).max().unwrap_or(0);
        if lanes > 0 {
            let mut marks = vec![' '; lanes];
            for (l, on) in &at {
                if let Some(m) = marks.get_mut(l.lane) {
                    *m = if *on { '\u{25cf}' } else { '\u{2506}' };
                }
            }
            if !text.is_empty() {
                text.push(' ');
            }
            text.extend(marks);
        }
        text
    }
}

/// One row of a strip's plan in physical order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlanItem<'a> {
    /// An accessory in a gap: `start`, `end` or the label it follows.
    Gap { gap: &'a str, acc: crate::terminal_strips::accessories::Acc },
    /// A position (its index in [`StripData::rows`]).
    Term(usize, &'a TermRow),
}

/// The strips of a project (see the module's update map).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Strips {
    pub strips: BTreeMap<DeviceId, StripData>,
    by_conductor: BTreeMap<WireId, BTreeSet<DeviceId>>,
    by_far: BTreeMap<DeviceId, BTreeSet<DeviceId>>,
    by_page: BTreeMap<PageId, BTreeSet<DeviceId>>,
    by_cable: BTreeMap<CableId, BTreeSet<DeviceId>>,
    /// The strips reading each project `strip_*` key (no value of their own).
    inherits: BTreeMap<String, BTreeSet<DeviceId>>,
    /// Placed accessories with no part number, over every strip (one info row).
    pub acc_no_part: usize,
    /// The fuse terminal devices (the `fuse-device` Info row).
    pub fuse_devices: BTreeSet<DeviceId>,
    /// How many strips the last update recomputed (tests, bench).
    pub last_work: usize,
    /// Kept totals: every strip's plan sheets, and BOM rows per
    /// location.
    pub sheets_total: usize,
    pub bom_rows: BTreeMap<Option<String>, usize>,
    /// Kept Checks rows of the strips (for the badge): `terminal-block-twice` errors and
    /// `too-many-terminal-blocks` warnings.
    pub check_errors: usize,
    pub check_warnings: usize,
    /// Bumped by an update that changed a strip, and the value at each strip's last change: what
    /// a copy of the plan (Reports ▸ Terminals) is keyed on.
    pub revision: u64,
    changed_at: BTreeMap<DeviceId, u64>,
    /// The strips whose data the last update changed (added, removed or different): what a shown
    /// terminal plan page depends on.
    pub last_changed: BTreeSet<DeviceId>,
}

fn add_to<K: Ord + Clone>(map: &mut BTreeMap<K, BTreeSet<DeviceId>>, keys: &BTreeSet<K>, c: DeviceId) {
    for k in keys {
        map.entry(k.clone()).or_default().insert(c);
    }
}

fn remove_from<K: Ord + Clone>(map: &mut BTreeMap<K, BTreeSet<DeviceId>>, keys: &BTreeSet<K>, c: DeviceId) {
    for k in keys {
        if let Some(set) = map.get_mut(k) {
            set.remove(&c);
            if set.is_empty() {
                map.remove(k);
            }
        }
    }
}

/// What a recompute of the strips needs to know beyond the document.
pub struct StripScope<'a> {
    pub devices: &'a BTreeSet<DeviceId>,
    pub wires: &'a BTreeSet<WireId>,
    pub cables: &'a BTreeSet<CableId>,
    pub pages: &'a BTreeSet<PageId>,
    /// Project `strip_*` keys that changed.
    pub project_keys: &'a BTreeSet<String>,
}

impl Strips {
    /// Every strip, from scratch.
    pub fn build(doc: &Document, o: &ReportModels, cx: &PageCx) -> Strips {
        let mut s = Strips::default();
        for c in doc.devices().keys() {
            if strip_of(doc, *c).is_some() {
                s.set(doc, *c, o, cx);
            }
            if fuse_terminal_block_of(doc, *c).is_some() {
                s.fuse_devices.insert(*c);
            }
        }
        s
    }

    /// The strips that read project key `key` (no value of their own).
    pub fn inheriting(&self, key: &str) -> Vec<DeviceId> {
        self.inherits.get(key).map(|s| s.iter().copied().collect()).unwrap_or_default()
    }

    /// Keep the fuse terminal devices among `devices` (the ops' devices) up to date.
    /// Returns whether the set changed.
    pub fn note_devices(&mut self, doc: &Document, devices: &BTreeSet<DeviceId>) -> bool {
        let mut changed = false;
        for c in devices {
            changed |= if fuse_terminal_block_of(doc, *c).is_some() { self.fuse_devices.insert(*c) } else { self.fuse_devices.remove(c) };
        }
        changed
    }

    /// The strips `scope` touches, before recomputing.
    pub fn touched(&self, doc: &Document, scope: &StripScope<'_>) -> BTreeSet<DeviceId> {
        let mut out = BTreeSet::new();
        for c in scope.devices {
            if self.strips.contains_key(c) || strip_of(doc, *c).is_some() {
                out.insert(*c);
            }
            out.extend(self.by_far.get(c).into_iter().flatten().copied());
        }
        let mut wires: BTreeSet<WireId> = scope.wires.clone();
        for k in scope.cables {
            out.extend(self.by_cable.get(k).into_iter().flatten().copied());
            if let Some(cab) = doc.cables().get(k) {
                wires.extend(cab.cores.iter().filter_map(|m| m.wire()));
            }
        }
        for w in &wires {
            out.extend(self.by_conductor.get(w).into_iter().flatten().copied());
            if let Some(cond) = doc.wires().get(w) {
                out.extend(cond.ends.iter().filter_map(|t| strip_of_connection_point(doc, *t)));
            }
        }
        for p in scope.pages {
            out.extend(self.by_page.get(p).into_iter().flatten().copied());
        }
        // A project level separator or level names relabel the strips inheriting them.
        for k in scope.project_keys.iter().filter(|k| crate::defaults::terminal_blocks::is_label_key(k)) {
            out.extend(self.inherits.get(k).into_iter().flatten().copied());
        }
        out
    }

    /// The strips reading a changed project `strip_*` key (no value of their own), not
    /// already in `full`: only their accessories need recomputing.
    pub fn touched_acc(&self, project_keys: &BTreeSet<String>, full: &BTreeSet<DeviceId>) -> BTreeSet<DeviceId> {
        project_keys.iter().flat_map(|k| self.inherits.get(k).into_iter().flatten().copied()).filter(|c| !full.contains(c)).collect()
    }

    /// Recompute only the accessories of `which` (a project default changes
    /// part data, never terminal rows). Adds to [`Strips::last_changed`] and `last_work`.
    pub fn recompute_acc(&mut self, doc: &Document, which: &BTreeSet<DeviceId>) {
        let mut changed = false;
        for c in which {
            let Some(old) = self.strips.get(c) else { continue };
            let Some(strip) = doc.devices().get(c) else { continue };
            let mut acc = crate::terminal_strips::accessories::strip_acc(doc, &strip.properties, &old.rows);
            // A project part or description default reaches the type rows too.
            let mut d = old.clone();
            acc.inherits.extend(d.refresh_types(doc, strip));
            // The level label keys a strip inherits are not this pass's (they rebuild the strip).
            acc.inherits.extend(old.acc.inherits.iter().filter(|k| crate::defaults::terminal_blocks::is_label_key(k)).cloned());
            if acc == old.acc && d.types == old.types {
                continue;
            }
            d.acc = acc;
            d.finish();
            self.remove(*c);
            self.insert(*c, d);
            self.last_changed.insert(*c);
            changed = true;
        }
        if changed {
            self.revision += 1;
            for c in which.iter().filter(|c| self.last_changed.contains(c)) {
                self.changed_at.insert(*c, self.revision);
            }
        }
        self.last_work += which.len();
    }

    /// Recompute `which` on `doc`.
    pub fn recompute(&mut self, doc: &Document, which: &BTreeSet<DeviceId>, o: &ReportModels, cx: &PageCx) {
        self.last_changed.clear();
        for c in which {
            let before = self.strips.get(c).cloned();
            self.set(doc, *c, o, cx);
            if self.strips.get(c) != before.as_ref() {
                self.last_changed.insert(*c);
            }
        }
        if !self.last_changed.is_empty() {
            self.revision += 1;
            for c in &self.last_changed {
                self.changed_at.insert(*c, self.revision);
            }
        }
        self.last_work = which.len();
    }

    fn remove(&mut self, c: DeviceId) {
        let Some(old) = self.strips.remove(&c) else { return };
        self.sheets_total = self.sheets_total.saturating_sub(old.sheets);
        let (er, wa) = old.check_rows();
        self.check_errors = self.check_errors.saturating_sub(er);
        self.check_warnings = self.check_warnings.saturating_sub(wa);
        if let Some(n) = self.bom_rows.get_mut(&old.location) {
            *n = n.saturating_sub(old.bom_rows);
            if *n == 0 {
                self.bom_rows.remove(&old.location);
            }
        }
        remove_from(&mut self.by_conductor, &old.wires, c);
        remove_from(&mut self.by_far, &old.far, c);
        remove_from(&mut self.by_page, &old.pages, c);
        remove_from(&mut self.by_cable, &old.cables, c);
        remove_from(&mut self.inherits, &old.acc.inherits, c);
        self.acc_no_part = self.acc_no_part.saturating_sub(old.acc.no_part);
    }

    fn set(&mut self, doc: &Document, c: DeviceId, o: &ReportModels, cx: &PageCx) {
        self.remove(c);
        if strip_of(doc, c).is_none() {
            return;
        }
        let d = strip_data(doc, c, o, cx);
        self.insert(c, d);
    }

    fn insert(&mut self, c: DeviceId, d: StripData) {
        add_to(&mut self.by_conductor, &d.wires, c);
        add_to(&mut self.by_far, &d.far, c);
        add_to(&mut self.by_page, &d.pages, c);
        add_to(&mut self.by_cable, &d.cables, c);
        add_to(&mut self.inherits, &d.acc.inherits, c);
        self.acc_no_part += d.acc.no_part;
        self.sheets_total += d.sheets;
        let (er, wa) = d.check_rows();
        self.check_errors += er;
        self.check_warnings += wa;
        if d.bom_rows > 0 {
            *self.bom_rows.entry(d.location.clone()).or_default() += d.bom_rows;
        }
        self.strips.insert(c, d);
    }

    /// The strips sorted by tag naturally.
    pub fn sorted(&self) -> Vec<&StripData> {
        let mut v: Vec<&StripData> = self.strips.values().collect();
        v.sort_by(|a, b| crate::locations::natural_key(&a.tag).cmp(&crate::locations::natural_key(&b.tag)).then(a.component.cmp(&b.component)));
        v
    }

    /// Whether `other` holds the same strips and indexes (verify mode: the kept model against a
    /// rebuild).
    pub fn same_as(&self, other: &Strips) -> bool {
        self.strips == other.strips
            && self.by_conductor == other.by_conductor
            && self.by_far == other.by_far
            && self.by_page == other.by_page
            && self.by_cable == other.by_cable
            && self.inherits == other.inherits
            && self.acc_no_part == other.acc_no_part
            && self.fuse_devices == other.fuse_devices
            && self.sheets_total == other.sheets_total
            && self.check_errors == other.check_errors
            && self.check_warnings == other.check_warnings
            && self.bom_rows == other.bom_rows
    }

    /// The revision at which strip `c` last changed (0: not since the build); with `None`, the
    /// revision of any strip's change.
    pub fn changed_at(&self, c: Option<DeviceId>) -> u64 {
        match c {
            Some(c) => self.changed_at.get(&c).copied().unwrap_or(0),
            None => self.revision,
        }
    }

    pub fn get(&self, c: DeviceId) -> Option<&StripData> {
        self.strips.get(&c)
    }

    /// How many strips.
    pub fn len(&self) -> usize {
        self.strips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strips.is_empty()
    }
}

/// Which way a route leaves a point: `true` for up or left (inside).
fn leaves_inward(from: Pt, next: Pt) -> bool {
    let (dx, dy) = (next.x - from.x, next.y - from.y);
    if dy.abs() >= dx.abs() { dy > 0.0 } else { dx < 0.0 }
}

/// The direction of the route leaving conductor `w`'s end `end` (its connection point) on
/// `page`: inside (`true`) or not; `None` when no route lands there on that page. The route
/// carrying `w` (one running to its other end) is preferred over another route at the point.
pub(super) fn geometric_inside(doc: &Document, w: WireId, end: End, page: PageId) -> Option<bool> {
    let wire = doc.wires().get(&w)?;
    let t = wire.end(end);
    let far = ecad_model::RouteEnd::Point(wire.end(end.other()));
    let mut at: Vec<&ecad_model::Route> = doc.routes_at(t).filter_map(|r| doc.routes().get(&r)).filter(|r| r.page == page).collect();
    at.sort_by_key(|r| !r.ends.contains(&far));
    at.into_iter().find_map(|r| {
        let v = &r.points;
        let (a, b) =
            if r.ends[0] == ecad_model::RouteEnd::Point(t) { (*v.first()?, *v.get(1)?) } else { (*v.last()?, *v.get(v.len().checked_sub(2)?)?) };
        Some(leaves_inward(a, b))
    })
}

/// The cell of a terminal's conductor's far end, worked out like the wire report's.
fn far_cell(doc: &Document, o: &ReportModels, w: WireId, far: End, cx: &PageCx) -> (Option<String>, Option<DeviceId>) {
    let Some(cond) = doc.wires().get(&w) else { return (None, None) };
    let t = cond.end(far);
    let comp = doc.connection_points().get(&t).and_then(|x| x.device());
    let cell = o.end_cell_of(doc, Some(t), None, cx);
    (cell.text(doc.settings().reference_style()), comp)
}

fn clean(v: Option<impl AsRef<str>>) -> Option<String> {
    v.map(|s| s.as_ref().trim().to_string()).filter(|s| !s.is_empty())
}

/// One strip's data on `doc`.
fn strip_data(doc: &Document, c: DeviceId, o: &ReportModels, cx: &PageCx) -> StripData {
    use crate::terminal_strips::terminal_blocks as tt;
    let strip = doc.devices().get(&c).cloned().unwrap_or_default();
    let props = &strip.properties;
    let device = o.device(c);
    let location = match device {
        Some(d) => d.location.value.clone(),
        None => crate::locations::resolve(doc, c).value,
    };
    let mut d = StripData {
        component: c,
        tag: doc.devices().get(&c).and_then(|d| d.tag_shown()).unwrap_or_else(|| "?".into()),
        location: location.clone(),
        offer: props.get(OFFER_KEY).and_then(|v| parse_offer(v).ok()),
        part_number: clean(strip.part_number.as_ref()),
        maker: clean(strip.manufacturer.as_ref()),
        part_number_pe: clean(props.get(PE_PART_KEY)),
        maker_pe: clean(props.get(PE_MAKER_KEY)),
        description: clean(strip.description.as_ref()),
        drawn: 0,
        drawn_pe: 0,
        spare: 0,
        spare_pe: 0,
        rows: Vec::new(),
        twice: Vec::new(),
        types: Vec::new(),
        mixed: Vec::new(),
        level_missing: Vec::new(),
        no_rating: 0,
        pos_of_row: Vec::new(),
        mixed_types: BTreeMap::new(),
        sheets: 1,
        bom_rows: 0,
        acc: crate::terminal_strips::accessories::StripAcc::default(),
        wires: BTreeSet::new(),
        far: BTreeSet::new(),
        pages: BTreeSet::new(),
        cables: BTreeSet::new(),
    };
    let drawn = drawn_at(doc, c);
    let shown_at: BTreeMap<ConnectionPointId, (RepresentationId, bool, Option<Pt>)> =
        drawn.iter().map(|(t, rid, pe, at)| (*t, (*rid, *pe, *at))).collect();
    let mine: BTreeSet<ConnectionPointId> = doc.connection_points_of(c).map(|(t, _)| t).collect();
    let mut grids: BTreeMap<PageId, ecad_model::document::sheet::Grid> = BTreeMap::new();
    let mut rows: Vec<TermRow> = Vec::new();
    // Bridges, for groups: union of terminals joined by a bridge.
    let mut parent: BTreeMap<ConnectionPointId, ConnectionPointId> = mine.iter().map(|t| (*t, *t)).collect();
    fn find(p: &mut BTreeMap<ConnectionPointId, ConnectionPointId>, t: ConnectionPointId) -> ConnectionPointId {
        let mut x = t;
        // Bounded by the strip's terminals.
        for _ in 0..=p.len() {
            match p.get(&x) {
                Some(n) if *n != x => x = *n,
                _ => break,
            }
        }
        x
    }
    let mut bridged: BTreeSet<ConnectionPointId> = BTreeSet::new();
    let separator = tt::separator(doc, c);
    let mut names_of: BTreeMap<crate::symbols::StarterKey, Vec<String>> = BTreeMap::new();
    let mut inherits: BTreeSet<String> = BTreeSet::new();
    let own = |k: &str| props.get(k).is_some_and(|v| !v.trim().is_empty());
    // The conductors on one terminal, sorted into inside and outside cells (two-point levels
    // by their point).
    let mut cells = |t: ConnectionPointId, side: Option<bool>, page: Option<PageId>, row: &mut TermRow, d: &mut StripData| {
        for (w, end) in doc.ends_at(t) {
            d.wires.insert(w);
            let Some(cond) = doc.wires().get(&w) else { continue };
            let far_end = match end {
                End::A => End::B,
                End::B => End::A,
            };
            let o2 = cond.end(far_end);
            if o2 != t && mine.contains(&o2) {
                // A bridge.
                bridged.insert(t);
                bridged.insert(o2);
                let (ra, rb) = (find(&mut parent, t), find(&mut parent, o2));
                if ra != rb {
                    parent.insert(ra.max(rb), ra.min(rb));
                }
                continue;
            }
            let (target, far) = far_cell(doc, o, w, far_end, cx);
            if let Some(f) = far {
                d.far.insert(f);
            }
            let cable = doc.cables_of(w).into_iter().next().map(|k| {
                d.cables.insert(k);
                let tag = doc.cables().get(&k).and_then(|x| clean(x.tag_shown().as_deref()));
                (k, tag)
            });
            let core =
                cable.as_ref().and_then(|(k, _)| doc.cables().get(k)?.cores.iter().find(|m| m.wire() == Some(w))?.label_str().map(str::to_string));
            let cell = SideCell { conductor: w, target, wire: o.wire(w).and_then(|r| r.wire_number.clone()), cable, core, far };
            // Cable core → outside; far device's location against the strip's; geometry.
            let inside = match side {
                Some(s) => s,
                None if cell.cable.is_some() => false,
                None => {
                    let far_loc = far.filter(|f| crate::locations::is_device(doc, *f)).and_then(|f| match o.device(f) {
                        Some(x) => x.location.value.clone(),
                        None => crate::locations::resolve(doc, f).value,
                    });
                    match (&location, far_loc) {
                        (Some(a), Some(b)) => *a == b,
                        _ => page.and_then(|p| geometric_inside(doc, w, end, p)).unwrap_or(true),
                    }
                }
            };
            if inside {
                row.inside.push(cell);
            } else {
                row.outside.push(cell);
            }
        }
    };
    let positions = tt::positions(doc, c);
    let mut drawn_by_type: [usize; tt::TYPE_COUNT] = [0; tt::TYPE_COUNT];
    for pos in &positions {
        let ty = pos.ty;
        if let Some(n) = drawn_by_type.get_mut(tt::index_of(ty)) {
            *n += 1;
        }
        if let Some(t) = pos.mixed.first() {
            d.mixed.push((pos.label.clone(), *t));
            d.mixed_types.insert(*t, tt::type_of_connection_point(doc, *t).word);
        }
        let fuse_rating = match &strip.device_type {
            ecad_model::DeviceType::TerminalStrip(s) => s.fuse_rating(&pos.label),
            _ => None,
        };
        if ty.code == "fuse" && fuse_rating.is_none() {
            d.no_rating += 1;
        }
        let names = names_of.entry(ty.key).or_insert_with(|| tt::level_names(doc, c, ty)).clone();
        if ty.multi() {
            if !own(crate::defaults::terminal_blocks::SEPARATOR.key) {
                inherits.insert(crate::defaults::terminal_blocks::SEPARATOR.key.to_string());
            }
            if let Some(e) = crate::defaults::terminal_blocks::levels_entry(ty.code)
                && !own(e.key)
            {
                inherits.insert(e.key.to_string());
            }
        }
        let rating = ty.breaks.then_some(fuse_rating).flatten();
        let mut seen_levels: BTreeSet<u32> = BTreeSet::new();
        // One row per level, free levels included; a level drawn twice adds a row (and a Checks row).
        let max_level = ty.count().max(pos.levels.keys().copied().max().unwrap_or(1));
        for n in 1..=max_level {
            let level_name = names.get(usize::try_from(n.saturating_sub(1)).unwrap_or(0)).cloned().unwrap_or_else(|| n.to_string());
            let derived = if ty.multi() { format!("{}{separator}{level_name}", pos.label) } else { pos.label.clone() };
            let over = clean(props.get(&tt::label_key(&pos.label, n)));
            let terms: Vec<ConnectionPointId> = pos.levels.get(&n).cloned().unwrap_or_default();
            if terms.is_empty() {
                if n > ty.count() {
                    continue;
                }
                let mut r = TermRow::blank(pos.label.clone(), ty, n, level_name);
                r.label_overridden = over.is_some();
                r.shown = over.unwrap_or(derived);
                r.free = true;
                r.rating = rating.clone();
                rows.push(r);
                continue;
            }
            // Split into drawings: in/out pairs for a two-point level, one terminal otherwise.
            let mut drawings: Vec<(ConnectionPointId, Option<ConnectionPointId>)> = Vec::new();
            if ty.breaks {
                let ins: Vec<ConnectionPointId> = terms.iter().copied().filter(|t| tt::level_of(doc, *t).1 != Some('o')).collect();
                let outs: Vec<ConnectionPointId> = terms.iter().copied().filter(|t| tt::level_of(doc, *t).1 == Some('o')).collect();
                for (i, t) in ins.iter().enumerate() {
                    drawings.push((*t, outs.get(i).copied()));
                }
                for t in outs.iter().skip(ins.len()) {
                    drawings.push((*t, None));
                }
            } else {
                drawings.extend(terms.iter().map(|t| (*t, None)));
            }
            for (k, (t, out)) in drawings.into_iter().enumerate() {
                let (rid, pe, at) = match shown_at.get(&t).or_else(|| out.and_then(|x| shown_at.get(&x))) {
                    Some((rid, pe, at)) => (Some(*rid), *pe, *at),
                    None => (None, false, None),
                };
                let page = rid.and_then(|r| doc.representations().get(&r)).map(|r| r.page);
                if let Some(p) = page {
                    d.pages.insert(p);
                }
                let mut row = TermRow::blank(pos.label.clone(), ty, n, level_name.clone());
                row.connection_point = Some(t);
                row.out = out;
                row.pe = pe || ty.pe_level == Some(n);
                row.label_overridden = over.is_some();
                row.shown = over.clone().unwrap_or_else(|| derived.clone());
                row.rating = rating.clone();
                row.internal_bridge = ty
                    .bridges
                    .iter()
                    .position(|g| g.iter().any(|key| ecad_model::connectivity::jumper_bars::level_of_key(Some(key)).is_some_and(|(m, _)| m == n)))
                    .map(|i| i + 1);
                row.place = page.zip(at).and_then(|(p, a)| {
                    let grid = grids.entry(p).or_insert_with(|| ecad_model::document::sheet::Grid::of(doc, p));
                    ecad_model::connectivity::derive::place_on(grid, cx.number(p)?, a)
                });
                row.go = page.zip(at);
                if ty.breaks {
                    cells(t, Some(true), page, &mut row, &mut d);
                    if let Some(x) = out {
                        cells(x, Some(false), page, &mut row, &mut d);
                    }
                } else {
                    cells(t, None, page, &mut row, &mut d);
                }
                if k > 0 || !seen_levels.insert(n) {
                    d.twice.push((row.shown.clone(), t));
                }
                rows.push(row);
            }
        }
    }
    // Bridge groups, numbered in plan order.
    let mut groups: BTreeMap<ConnectionPointId, usize> = BTreeMap::new();
    for r in &mut rows {
        let Some(t) = r.connection_point else { continue };
        let t = if bridged.contains(&t) {
            t
        } else {
            match r.out.filter(|x| bridged.contains(x)) {
                Some(x) => x,
                None => continue,
            }
        };
        let root = find(&mut parent, t);
        let n = groups.len() + 1;
        r.bridge_group = Some(*groups.entry(root).or_insert(n));
    }
    d.drawn = positions.len();
    d.drawn_pe = drawn_by_type.get(1).copied().unwrap_or(0);
    // Spare positions: labels 1…n no drawn position has, given to the offer's types in its
    // order (feed-through first), each with its type's levels.
    let mut spare_by_type: [usize; tt::TYPE_COUNT] = [0; tt::TYPE_COUNT];
    if let Some(off) = d.offer {
        let taken: BTreeSet<&str> = positions.iter().map(|p| p.label.as_str()).collect();
        let mut free: std::collections::VecDeque<u32> = (1..=off.positions).filter(|n| !taken.contains(n.to_string().as_str())).collect();
        let legacy_only = off.counts.iter().skip(2).all(|n| *n == 0) && positions.iter().all(|p| p.ty.legacy());
        let mut wanted: Vec<(usize, usize)> = Vec::new();
        if legacy_only {
            // With no type codes: the PE spares follow the last drawn PE terminal; feed-through the rest.
            let pe_wanted = (off.pe as usize).saturating_sub(d.drawn_pe).min(free.len());
            let last_pe = positions.iter().filter(|p| p.ty.code == "pe").filter_map(|p| p.label.parse::<u32>().ok()).max().unwrap_or(0);
            let spare: Vec<u32> = free.iter().copied().collect();
            let mut pe_set: BTreeSet<u32> = spare.iter().copied().filter(|n| *n > last_pe).take(pe_wanted).collect();
            for n in spare.iter().rev() {
                if pe_set.len() >= pe_wanted {
                    break;
                }
                pe_set.insert(*n);
            }
            let (ft, pe) = (tt::feedthrough(), tt::by_code("pe").unwrap_or_else(tt::feedthrough));
            for n in &spare {
                let ty = if pe_set.contains(n) { pe } else { ft };
                rows.push(TermRow::blank(n.to_string(), ty, 1, "1".into()));
                if let Some(x) = spare_by_type.get_mut(tt::index_of(ty)) {
                    *x += 1;
                }
            }
            free.clear();
        } else {
            for (i, t) in tt::TYPES.iter().enumerate() {
                let drawn_n = drawn_by_type.get(i).copied().unwrap_or(0);
                wanted.push((i, (off.of(t) as usize).saturating_sub(drawn_n)));
            }
        }
        for (i, want) in wanted {
            let Some(ty) = tt::TYPES.get(i) else { continue };
            // Spare multi-level positions read the separator and level names too.
            if want > 0 && ty.multi() {
                if !own(crate::defaults::terminal_blocks::SEPARATOR.key) {
                    inherits.insert(crate::defaults::terminal_blocks::SEPARATOR.key.to_string());
                }
                if let Some(e) = crate::defaults::terminal_blocks::levels_entry(ty.code)
                    && !own(e.key)
                {
                    inherits.insert(e.key.to_string());
                }
            }
            let names = names_of.entry(ty.key).or_insert_with(|| tt::level_names(doc, c, ty)).clone();
            for _ in 0..want {
                let Some(n) = free.pop_front() else { break };
                for lvl in 1..=ty.count() {
                    let name = names.get(usize::try_from(lvl - 1).unwrap_or(0)).cloned().unwrap_or_else(|| lvl.to_string());
                    let mut r = TermRow::blank(n.to_string(), ty, lvl, name.clone());
                    r.shown = if ty.multi() { format!("{n}{separator}{name}") } else { n.to_string() };
                    rows.push(r);
                }
                if let Some(x) = spare_by_type.get_mut(i) {
                    *x += 1;
                }
            }
        }
        d.spare = spare_by_type.iter().sum();
        d.spare_pe = spare_by_type.get(1).copied().unwrap_or(0);
        rows.sort_by_cached_key(|r| (crate::references::devices::table::natural_key(&r.label), r.level, r.connection_point));
    }
    // Part data per type: strip, then project, then the registry's description.
    for (i, ty) in tt::TYPES.iter().enumerate() {
        let (dn, sp) = (drawn_by_type.get(i).copied().unwrap_or(0), spare_by_type.get(i).copied().unwrap_or(0));
        if dn + sp == 0 {
            continue;
        }
        d.types.push(type_parts(doc, &strip, ty, dn, sp, &mut inherits));
    }
    // Each row's position index.
    let mut n = 0usize;
    for (i, r) in rows.iter().enumerate() {
        if i > 0 && rows.get(i - 1).is_some_and(|p| p.label != r.label) {
            n += 1;
        }
        d.pos_of_row.push(n);
    }
    // Jumper bars naming levels their positions lack (`level-missing`).
    let count_of: BTreeMap<&str, u32> = positions.iter().map(|p| (p.label.as_str(), p.ty.count())).collect();
    let order: Vec<String> = positions.iter().map(|p| p.label.clone()).collect();
    for (n, p) in ecad_model::connectivity::jumper_bars::of(props) {
        let mut items = Vec::new();
        for item in &p.0 {
            let Some(sp) = item.span() else { continue };
            let missing = match &sp {
                ecad_model::connectivity::jumper_bars::Span::Levels { position, hi, .. } => count_of.get(position.as_str()).is_some_and(|c| hi > c),
                ecad_model::connectivity::jumper_bars::Span::Positions { level, .. } => {
                    let one = ecad_model::connectivity::jumper_bars::Positions(vec![item.clone()]);
                    one.select(&order).iter().any(|l| count_of.get(l.as_str()).is_some_and(|c| level > c))
                }
            };
            if missing {
                items.push(match item {
                    ecad_model::connectivity::jumper_bars::Item::One(x) => x.clone(),
                    ecad_model::connectivity::jumper_bars::Item::Range(a, b) => format!("{a}-{b}"),
                });
            }
        }
        if !items.is_empty() {
            d.level_missing.push((n, items));
        }
    }
    d.rows = rows;
    d.acc = crate::terminal_strips::accessories::strip_acc(doc, props, &d.rows);
    d.acc.inherits.extend(inherits);
    d.finish();
    d
}

/// The strip key of a type's part field: feed-through keeps `part_number`, `maker`; every
/// other type `<field>.<code>` (`part_number.lnpe`; `part_number.pe` follows this rule).
pub fn type_key(t: &crate::terminal_strips::terminal_blocks::TermType, field: &str) -> String {
    if t.code == "ft" { field.to_string() } else { format!("{field}.{}", t.code) }
}

/// One type's BOM part data on `strip`: the strip's value, else the project's
/// `strip_<code>_<field>` (recorded in `inherits`), else the registry's description. A
/// feed-through's part is the strip's own fields ([`type_key`] is then the field's key).
fn type_parts(
    doc: &Document, strip: &ecad_model::Device, ty: &'static crate::terminal_strips::terminal_blocks::TermType, drawn: usize, spare: usize,
    inherits: &mut BTreeSet<String>,
) -> TypeCount {
    let mut get = |field: &str| -> Option<String> {
        if let Some(v) = clean(strip.get(&type_key(ty, field))) {
            return Some(v);
        }
        let pk = format!("strip_{}_{field}", ty.code);
        inherits.insert(pk.clone());
        clean(doc.display().get(&pk))
    };
    let own_part = clean(strip.get(&type_key(ty, "part_number"))).is_some();
    let part_number = get("part_number");
    let maker = get("maker");
    // A strip's own part number never shows the project's article number.
    let article_number = if own_part || part_number.is_none() {
        clean(strip.get(&type_key(ty, crate::reports::bom::ARTICLE_KEY)))
    } else {
        get(crate::reports::bom::ARTICLE_KEY)
    };
    let own_description = get("description").or_else(|| if ty.code == "ft" { clean(strip.description.as_ref()) } else { None });
    let description = own_description.unwrap_or_else(|| {
        let e = crate::defaults::terminal_blocks::description_entry(ty.code);
        if let Some(e) = e {
            inherits.insert(e.key.to_string());
        }
        e.map_or_else(|| ty.name.to_string(), |e| crate::defaults::resolve(doc, e, crate::defaults::Scope::project()).text())
    });
    TypeCount { ty, drawn, spare, part_number, maker, article_number, description }
}

impl StripData {
    /// Its kept sheet and BOM row counts, from its rows and accessories.
    fn finish(&mut self) {
        self.sheets = (plan_lines(self) - 1).div_ceil(crate::pages::generated::rows_per_sheet() - 1).max(1);
        self.bom_rows = self.types.iter().filter(|t| t.qty() > 0).count() + self.acc.bom.len();
    }

    /// Recompute its part data per type (a project default changed).
    fn refresh_types(&mut self, doc: &Document, strip: &ecad_model::Device) -> BTreeSet<String> {
        let mut inherits = BTreeSet::new();
        for t in &mut self.types {
            *t = type_parts(doc, strip, t.ty, t.drawn, t.spare, &mut inherits);
        }
        inherits
    }
}
