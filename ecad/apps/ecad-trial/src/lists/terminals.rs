//! The terminal manager as a column set (brief 039 step 4): design 036 §9's row model laid out
//! as a terminal plan, mirrored: inside columns right-aligned towards the strip, the strip in the
//! middle, outside columns left-aligned away from it. Only 036's editable fields are inputs:
//! Position and Label (`termlabel`), Rating on a fuse position (`setprop rating.<position>`); the
//! links are lanes beside the terminals. Everything else is read.
//!
//! **Update map.** The rows are the kept report models' strip rows (`Outputs::strips`, kept from
//! each change's ops, design 036 §11). Per strip, the revision it last changed at
//! (`Strips::changed_at`) is compared with the one the table last read: only strips that changed
//! (or appeared or went) have their rows re-read and patched. Every other commit: nothing. Full
//! rebuild only on first show, a reload of the models (`build_id`), or a new strip filter,
//! accessories switch or reference style.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ecad_cadcraft::outputs::Outputs;
use ecad_cadcraft::strips::{PlanItem, SideCell, StripData, TermRow};
use ecad_model::ComponentId;
use ecad_ui::table::{Align, Cell, Changed, Column, ColumnId, Lane, LaneCell, Table, TableEvent, TableSource, Width};
use serde_json::json;

use super::ListAction;

/// A row: its strip and its place in the strip's plan.
pub type TermKey = (ComponentId, u32);

pub const STRIP: ColumnId = ColumnId(0);
pub const IN_TARGET: ColumnId = ColumnId(1);
pub const IN_WIRE: ColumnId = ColumnId(2);
pub const POSITION: ColumnId = ColumnId(3);
pub const LABEL: ColumnId = ColumnId(4);
pub const LEVEL: ColumnId = ColumnId(5);
pub const TYPE: ColumnId = ColumnId(6);
pub const RATING: ColumnId = ColumnId(7);
pub const LINKS: ColumnId = ColumnId(8);
pub const OUT_WIRE: ColumnId = ColumnId(9);
pub const CABLE: ColumnId = ColumnId(10);
pub const CORE: ColumnId = ColumnId(11);
pub const OUT_TARGET: ColumnId = ColumnId(12);
pub const PLACE: ColumnId = ColumnId(13);

/// The columns: strip, inside (right-aligned), the strip's own fields, outside (left-aligned),
/// place.
pub fn columns() -> Vec<Column> {
    vec![
        Column::read(STRIP.0, "Strip", Width::Initial(56.0)).groupable(),
        Column::read(IN_TARGET.0, "Inside target", Width::Initial(110.0)).align(Align::Right),
        Column::read(IN_WIRE.0, "Inside wire", Width::Initial(80.0)).align(Align::Right),
        Column::set(POSITION.0, "Position", Width::Initial(72.0)).align(Align::Center),
        Column::set(LABEL.0, "Label", Width::Initial(72.0)).align(Align::Center),
        Column::read(LEVEL.0, "Level", Width::Initial(56.0)).align(Align::Center),
        Column::read(TYPE.0, "Type", Width::Initial(96.0)).align(Align::Center).groupable(),
        Column::set(RATING.0, "Rating", Width::Initial(64.0)).align(Align::Center),
        Column::read(LINKS.0, "Links", Width::Initial(64.0)).unsorted(),
        Column::read(OUT_WIRE.0, "Outside wire", Width::Initial(80.0)),
        Column::read(CABLE.0, "Cable", Width::Initial(64.0)),
        Column::read(CORE.0, "Core", Width::Initial(48.0)),
        Column::read(OUT_TARGET.0, "Outside target", Width::Initial(110.0)),
        Column::read(PLACE.0, "Place", Width::Remainder),
    ]
}

/// One plan row, copied from the strip model.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanRow {
    /// A level of a position (036 §9's row).
    Term {
        tag: String,
        row: Box<TermRow>,
        /// The first row of its position (the Position cell is an input only there).
        first: bool,
        marker: bool,
        lanes: LaneCell,
        place: String,
    },
    /// An accessory in a gap (030 §7.1).
    Gap { tag: String, mark: String, words: String },
}

/// The terminal plan as a table source.
#[derive(Debug)]
pub struct TerminalsSource {
    cols: Vec<Column>,
    keys: Vec<TermKey>,
    rows: HashMap<TermKey, PlanRow>,
    /// Each strip's keys, in plan order.
    by_strip: BTreeMap<ComponentId, Vec<TermKey>>,
}

impl Default for TerminalsSource {
    fn default() -> Self {
        Self { cols: columns(), keys: Vec::new(), rows: HashMap::new(), by_strip: BTreeMap::new() }
    }
}

fn side(cells: &[SideCell], f: fn(&SideCell) -> Option<String>) -> String {
    cells.iter().map(|c| f(c).unwrap_or_else(|| "?".into())).collect::<Vec<_>>().join(", ")
}

/// Strip `s`'s plan rows.
fn plan_rows(s: &StripData, accessories: bool, style: ecad_model::ReferenceStyle) -> Vec<PlanRow> {
    let mut out = Vec::new();
    for item in s.plan(accessories) {
        match item {
            PlanItem::Gap { acc, .. } => {
                let words =
                    if acc == ecad_cadcraft::accessories::Acc::StripLabel { format!("strip label {}", s.tag) } else { acc.words().to_string() };
                out.push(PlanRow::Gap { tag: s.tag.clone(), mark: acc.mark().to_string(), words });
            }
            PlanItem::Term(i, r) => {
                let pi = s.position_index(i);
                let at = s.links_at(i);
                let n = at.iter().map(|(l, _)| l.lane + 1).max().unwrap_or(0);
                let mut lanes: Vec<Option<Lane>> = vec![None; n];
                for (l, on) in &at {
                    if let Some(slot) = lanes.get_mut(l.lane) {
                        *slot = Some(Lane { on: *on, up: pi > l.first, down: pi < l.last });
                    }
                }
                let text = r.bridge_group.map_or_else(String::new, |g| format!("\u{25cf} {g}"));
                out.push(PlanRow::Term {
                    tag: s.tag.clone(),
                    first: s.first_of_position(i),
                    marker: s.acc.markers.contains(&r.label),
                    lanes: LaneCell { lanes, text },
                    place: r.place.as_ref().map(|p| p.text(style)).unwrap_or_default(),
                    row: Box::new(r.clone()),
                });
            }
        }
    }
    out
}

impl TerminalsSource {
    pub fn row(&self, k: TermKey) -> Option<&PlanRow> {
        self.rows.get(&k)
    }

    /// Replace strip `c`'s rows with `rows` (none: the strip went). Returns the keys touched: its
    /// old and new rows.
    fn set_strip(&mut self, c: ComponentId, rows: Vec<PlanRow>) -> Vec<TermKey> {
        let mut touched = self.by_strip.remove(&c).unwrap_or_default();
        for k in &touched {
            self.rows.remove(k);
        }
        let keys: Vec<TermKey> = (0..rows.len()).map(|i| (c, u32::try_from(i).unwrap_or(u32::MAX))).collect();
        for (k, r) in keys.iter().zip(rows) {
            self.rows.insert(*k, r);
        }
        touched.extend(keys.iter().copied());
        if !keys.is_empty() {
            self.by_strip.insert(c, keys);
        }
        touched.sort();
        touched.dedup();
        touched
    }

    /// The strips in `order` (tag order), their keys laid end to end.
    fn order(&mut self, order: &[ComponentId]) {
        self.keys = order.iter().filter_map(|c| self.by_strip.get(c)).flatten().copied().collect();
    }
}

impl TableSource for TerminalsSource {
    type Key = TermKey;

    fn columns(&self) -> &[Column] {
        &self.cols
    }

    fn keys(&self) -> &[TermKey] {
        &self.keys
    }

    fn contains(&self, key: TermKey) -> bool {
        self.rows.contains_key(&key)
    }

    fn cell(&self, key: TermKey, col: ColumnId) -> Cell {
        let Some(r) = self.rows.get(&key) else { return Cell::Empty };
        let (tag, row, first, marker, lanes, place) = match r {
            PlanRow::Gap { tag, mark, words } => {
                return match col {
                    STRIP => Cell::read(tag.clone()),
                    POSITION => Cell::read(mark.clone()),
                    LABEL => Cell::read(words.clone()),
                    _ => Cell::Empty,
                };
            }
            PlanRow::Term { tag, row, first, marker, lanes, place } => (tag, row, *first, *marker, lanes, place),
        };
        let drawn = row.terminal.is_some();
        let cable = |c: &SideCell| c.cable.as_ref().map(|(_, t)| t.clone().unwrap_or_else(|| "?".into()));
        match col {
            STRIP => Cell::read(tag.clone()),
            IN_TARGET => Cell::read(side(&row.inside, |c| c.target.clone())),
            IN_WIRE => Cell::read(side(&row.inside, |c| c.wire.clone())),
            POSITION if first && drawn => Cell::Input(row.label.clone()),
            POSITION if first => Cell::read(format!("{}{}", row.label, if marker { " \u{25ad}" } else { "" })),
            POSITION => Cell::Empty,
            LABEL if drawn => Cell::Input(row.shown.clone()),
            LABEL => Cell::read(format!("{} ({})", row.shown, row.state())),
            LEVEL => Cell::read(row.level_name.clone()),
            TYPE => Cell::read(row.ty.name),
            RATING if row.breaks() && first => Cell::Input(row.rating.clone().unwrap_or_default()),
            RATING => Cell::Empty,
            LINKS => Cell::Lanes(lanes.clone()),
            OUT_WIRE => Cell::read(side(&row.outside, |c| c.wire.clone())),
            CABLE => Cell::read(row.outside.iter().filter_map(cable).collect::<Vec<_>>().join(", ")),
            CORE => Cell::read(
                row.outside.iter().filter(|c| c.cable.is_some()).map(|c| c.core.clone().unwrap_or_else(|| "?".into())).collect::<Vec<_>>().join(", "),
            ),
            OUT_TARGET => Cell::read(side(&row.outside, |c| c.target.clone())),
            PLACE => Cell::Read { text: place.clone(), go: row.go.is_some() },
            _ => Cell::Empty,
        }
    }

    fn row_label(&self, key: TermKey) -> String {
        match self.rows.get(&key) {
            Some(PlanRow::Term { tag, row, .. }) => format!("Terminal {tag}:{}", row.shown),
            Some(PlanRow::Gap { tag, words, .. }) => format!("Accessory {tag}: {words}"),
            None => String::new(),
        }
    }
}

/// The command line an edit of `col` on `row` (of strip `c`) runs, `value` trimmed.
pub fn edit_line(c: ComponentId, row: &TermRow, col: ColumnId, value: &str) -> Option<String> {
    let v = value.trim();
    let strip = c.hex();
    match col {
        POSITION => Some(format!("termlabel {}", json!({"strip": strip, "at": row.label, "position": v}))),
        LABEL => {
            let text = if v.is_empty() { serde_json::Value::Null } else { json!(v) };
            Some(format!("termlabel {}", json!({"strip": strip, "at": row.label, "level": row.level, "text": text})))
        }
        RATING => {
            let value = if v.is_empty() { serde_json::Value::Null } else { json!(v) };
            Some(format!("setprop {}", json!({"component": strip, "key": format!("rating.{}", row.label), "value": value})))
        }
        _ => None,
    }
}

/// Reports ▸ Terminals: the source, the table and what it last read.
#[derive(Debug)]
pub struct TerminalsView {
    pub source: TerminalsSource,
    pub table: Table<TermKey>,
    /// (project serial, models build, strip filter, accessories, reference style) of the build.
    built: Option<(u64, u64, Option<ComponentId>, bool, u64)>,
    /// The strips revision last read, and each strip's own.
    revision: u64,
    strips: BTreeMap<ComponentId, u64>,
    /// The cell whose edit is running (its refusal goes under it).
    pending: Option<(TermKey, ColumnId)>,
    /// Rebuilds and patched keys (tests).
    pub rebuilds: u64,
}

impl Default for TerminalsView {
    fn default() -> Self {
        let mut table = Table::new("ecad.terminals", "Terminals");
        table.empty = "No terminal strips yet.";
        Self { source: TerminalsSource::default(), table, built: None, revision: 0, strips: BTreeMap::new(), pending: None, rebuilds: 0 }
    }
}

impl TerminalsView {
    /// Forget what was read: the next sync rebuilds.
    pub fn invalidate(&mut self) {
        self.built = None;
    }

    /// Bring the view up to date with the models (the update map in the module docs).
    pub fn sync(&mut self, serial: u64, o: &Outputs, strip: Option<ComponentId>, accessories: bool, style: ecad_model::ReferenceStyle) {
        let key = (serial, o.build_id, strip, accessories, style as u64);
        let shown: Vec<&StripData> = match strip {
            Some(c) => o.strips.get(c).into_iter().collect(),
            None => o.strips.sorted(),
        };
        let order: Vec<ComponentId> = shown.iter().map(|s| s.component).collect();
        if self.built != Some(key) {
            self.source = TerminalsSource::default();
            for s in &shown {
                self.source.set_strip(s.component, plan_rows(s, accessories, style));
            }
            self.source.order(&order);
            self.strips = shown.iter().map(|s| (s.component, o.strips.changed_at(Some(s.component)))).collect();
            self.revision = o.strips.revision;
            self.built = Some(key);
            self.rebuilds += 1;
            self.table.update(&self.source, Changed::All);
            if strip.is_none() && self.table.view.group_by().is_none() && order.len() > 1 {
                self.table.view.set_group(&self.source, Some(STRIP));
            }
            return;
        }
        if o.strips.revision == self.revision {
            return;
        }
        self.revision = o.strips.revision;
        let mut touched = Vec::new();
        let now: BTreeSet<ComponentId> = order.iter().copied().collect();
        for s in &shown {
            let at = o.strips.changed_at(Some(s.component));
            if self.strips.get(&s.component) != Some(&at) {
                self.strips.insert(s.component, at);
                touched.extend(self.source.set_strip(s.component, plan_rows(s, accessories, style)));
            }
        }
        let gone: Vec<ComponentId> = self.strips.keys().copied().filter(|c| !now.contains(c)).collect();
        for c in gone {
            self.strips.remove(&c);
            touched.extend(self.source.set_strip(c, Vec::new()));
        }
        if touched.is_empty() {
            return;
        }
        self.source.order(&order);
        self.table.update(&self.source, Changed::Keys(touched));
    }

    /// The result of the last edit's command: a refusal shows under its cell.
    pub fn ran(&mut self, refusal: Option<String>) {
        if let (Some((k, col)), Some(msg)) = (self.pending.take(), refusal) {
            self.table.refuse(k, col, msg);
        }
    }

    /// Draw the table. Returns what it asks for.
    pub fn show(&mut self, ui: &mut eframe::egui::Ui, writable: bool) -> Vec<ListAction> {
        let mut out = Vec::new();
        for e in self.table.show(ui, &self.source) {
            match e {
                TableEvent::Commit { key, col, value } if writable => {
                    if let Some(PlanRow::Term { row, .. }) = self.source.row(key)
                        && let Some(line) = edit_line(key.0, row, col, &value)
                    {
                        self.pending = Some((key, col));
                        out.push(ListAction::Run(line));
                    }
                }
                TableEvent::GoTo { key, .. } | TableEvent::Open(key) => {
                    if let Some(PlanRow::Term { row, .. }) = self.source.row(key)
                        && let Some((pg, at)) = row.go
                    {
                        out.push(ListAction::GoTo(pg, at));
                    }
                }
                TableEvent::Export(csv) => super::copy_csv(ui, csv),
                TableEvent::Commit { .. } | TableEvent::Selected(_) | TableEvent::Button { .. } => {}
            }
        }
        out
    }
}
