//! Devices as a column set: tag and description set here (inline edits run
//! `setprop`, the same command, transaction and refusal as on the page); function, location
//! ("from box" where a box gives it), symbol, pages placed (a go-to), part and state read. Filter
//! chips: Provisional, Not on a page yet.
//!
//! **Update map.** The rows are the Devices list's entries ([`ecad_engine::device_list`]) plus
//! each device's resolved location. On each sync the project's device feed
//! (`device_changes_since`) names the touched devices; only their rows are re-read and patched.
//! A touched location box (its outline or value) or changed project settings re-read every row's
//! location (rebuild); pages added, removed or renumbered change every place (rebuild). Every
//! other commit: nothing. Full rebuild only on first show, a new project, or a feed that rolled
//! over.

use std::collections::{BTreeSet, HashMap};

use ecad_electrical::devices::device_list::DeviceEntry;
use ecad_model::DeviceId;
use ecad_widgets::prim::Tone;
use ecad_widgets::table::{Cell, Changed, Column, ColumnId, TableSource, Width};

pub const TAG: ColumnId = ColumnId(0);
pub const DESCRIPTION: ColumnId = ColumnId(1);
pub const FUNCTION: ColumnId = ColumnId(2);
pub const LOCATION: ColumnId = ColumnId(3);
pub const SYMBOL: ColumnId = ColumnId(4);
pub const PAGES: ColumnId = ColumnId(5);
pub const PART: ColumnId = ColumnId(6);
pub const STATE: ColumnId = ColumnId(7);
/// How many part lines the device's part set has.
pub const PARTS: ColumnId = ColumnId(8);

/// The chips.
pub(crate) const PROVISIONAL: &str = "Provisional";
pub(crate) const NOT_PLACED: &str = "Not on a page yet";

/// The property a set column writes.
pub fn property(col: ColumnId) -> Option<&'static str> {
    match col {
        TAG => Some("tag"),
        DESCRIPTION => Some("description"),
        _ => None,
    }
}

/// The column a property is shown in.
pub fn column_of(key: &str) -> Option<ColumnId> {
    match key {
        "tag" => Some(TAG),
        "description" => Some(DESCRIPTION),
        _ => None,
    }
}

/// The columns, in order. The state (provisional, not on a page yet,
/// fixed) is no column of its own: the tag shows a provisional one, On page says "not yet", and
/// the chips still filter by both.
pub fn columns() -> Vec<Column> {
    vec![
        Column::set(TAG.0, "Tag", Width::Initial(90.0)),
        Column::set(DESCRIPTION.0, "Description", Width::Initial(420.0)),
        Column::read(FUNCTION.0, "Function", Width::Initial(96.0)).groupable(),
        Column::read(LOCATION.0, "Location", Width::Initial(170.0)).groupable(),
        Column::read(PART.0, "Main part, order code", Width::Initial(200.0)).groupable(),
        Column::read(PARTS.0, "Parts", Width::Initial(56.0)).align(ecad_widgets::table::Align::Right),
        Column::read(SYMBOL.0, "Symbol", Width::Initial(200.0)).groupable(),
        Column::read(PAGES.0, "On page", Width::Remainder),
    ]
}

/// What the Main part column says of a device with no part.
pub(crate) const NO_PARTS: &str = "No parts yet";
/// What On page says of a device on no page.
pub const NOT_YET: &str = "not yet";

/// One device's row: its list entry and its location as shown.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceRow {
    pub entry: DeviceEntry,
    /// `+CP1`, `+CP1 (from box on 105)`, `?`.
    pub location: String,
    /// Its part set's lines (`ecad_electrical::reports::bom::part_set`).
    pub parts: usize,
}

/// The devices as a table source, in the Devices list's order.
#[derive(Debug)]
pub struct DevicesSource {
    cols: Vec<Column>,
    keys: Vec<DeviceId>,
    rows: HashMap<DeviceId, DeviceRow>,
    /// The search matches as the Devices filter does (`k1` finds `-K01`).
    pub search: bool,
}

impl Default for DevicesSource {
    fn default() -> Self {
        Self { cols: columns(), keys: Vec::new(), rows: HashMap::new(), search: true }
    }
}

fn location(doc: &ecad_model::Document, c: DeviceId, cache: &mut ecad_electrical::locations::BoxCache) -> String {
    ecad_electrical::locations::resolve_with(doc, c, cache).shown(doc)
}

fn row(doc: &ecad_model::Document, e: &DeviceEntry, cache: &mut ecad_electrical::locations::BoxCache) -> DeviceRow {
    // A part set lists the main part first; a device with only a main part has that one line.
    let set = ecad_electrical::reports::bom::part_set(doc, e.id).len();
    let main = e.properties.get(3).is_some_and(|p| p.as_deref().is_some_and(|p| !p.trim().is_empty()));
    DeviceRow { entry: e.clone(), location: location(doc, e.id, cache), parts: if set > 0 { set } else { usize::from(main) } }
}

impl DevicesSource {
    pub fn row(&self, c: DeviceId) -> Option<&DeviceRow> {
        self.rows.get(&c)
    }

    /// Whether `c` is a row.
    pub fn contains_key(&self, c: DeviceId) -> bool {
        self.rows.contains_key(&c)
    }

    /// Read every row from `entries` (the whole list: first show, rebuild).
    pub fn build(&mut self, doc: &ecad_model::Document, entries: &[DeviceEntry]) {
        let mut cache = ecad_electrical::locations::BoxCache::default();
        self.keys = entries.iter().map(|e| e.id).collect();
        self.rows = entries.iter().map(|e| (e.id, row(doc, e, &mut cache))).collect();
    }

    /// Re-read the rows of `touched` from `entries` (added, changed or removed), and the order.
    pub fn patch(&mut self, doc: &ecad_model::Document, entries: &[DeviceEntry], touched: &BTreeSet<DeviceId>) {
        let mut cache = ecad_electrical::locations::BoxCache::default();
        for c in touched {
            match entries.iter().find(|e| e.id == *c) {
                Some(e) => {
                    self.rows.insert(*c, row(doc, e, &mut cache));
                }
                None => {
                    self.rows.remove(c);
                }
            }
        }
        if !touched.is_empty() {
            self.keys = entries.iter().map(|e| e.id).collect();
        }
    }
}

impl TableSource for DevicesSource {
    type Key = DeviceId;

    fn columns(&self) -> &[Column] {
        &self.cols
    }

    fn keys(&self) -> &[DeviceId] {
        &self.keys
    }

    fn contains(&self, key: DeviceId) -> bool {
        self.rows.contains_key(&key)
    }

    fn cell(&self, key: DeviceId, col: ColumnId) -> Cell {
        let Some(r) = self.rows.get(&key) else { return Cell::Empty };
        let e = &r.entry;
        let prop = |i: usize| e.properties.get(i).cloned().flatten().unwrap_or_default();
        match col {
            TAG => match e.shown() {
                Some(t) if e.tag_state.is_provisional() => Cell::Provisional(t),
                Some(t) => Cell::Input(t),
                None => Cell::Input(String::new()),
            },
            DESCRIPTION => Cell::Input(prop(1)),
            FUNCTION => Cell::read(prop(5)),
            LOCATION => Cell::read(r.location.clone()),
            SYMBOL => {
                let mut names: Vec<&str> = Vec::new();
                for s in &e.symbols {
                    if !names.contains(&s.name.as_str()) {
                        names.push(&s.name);
                    }
                }
                // A copy whose project symbol has a newer version.
                if e.symbols.iter().any(|s| s.update) {
                    return Cell::Badge(format!("{} · update available", names.join(", ")), Tone::Provisional);
                }
                Cell::read(names.join(", "))
            }
            PAGES if e.symbols.is_empty() => Cell::read(NOT_YET),
            PAGES => Cell::Read { text: e.symbols.iter().map(|s| s.place.as_str()).collect::<Vec<_>>().join(", "), go: true },
            // The main part's order code; the maker is in the right panel.
            PART => {
                let part = prop(3);
                if part.is_empty() { Cell::Badge(NO_PARTS.into(), Tone::Warning) } else { Cell::read(part) }
            }
            PARTS => Cell::Number(r.parts as f64, ""),
            STATE => {
                if e.tag_state.is_provisional() {
                    Cell::Badge(PROVISIONAL.into(), Tone::Provisional)
                } else if e.symbols.is_empty() {
                    Cell::Badge(NOT_PLACED.into(), Tone::Warning)
                } else if e.tag_state == ecad_model::naming::tag::TagState::Fixed {
                    Cell::Badge("Fixed".into(), Tone::Neutral)
                } else {
                    Cell::Empty
                }
            }
            _ => Cell::Empty,
        }
    }

    fn row_label(&self, key: DeviceId) -> String {
        self.rows.get(&key).map(|r| crate::devices::panel::device_label(&r.entry)).unwrap_or_default()
    }

    fn chips(&self) -> &[&'static str] {
        &[PROVISIONAL, NOT_PLACED]
    }

    fn chip(&self, key: DeviceId, chip: usize) -> bool {
        let Some(e) = self.rows.get(&key).map(|r| &r.entry) else { return false };
        match chip {
            0 => e.tag_state.is_provisional(),
            1 => e.symbols.is_empty(),
            _ => true,
        }
    }

    fn matches(&self, key: DeviceId, query: &str) -> Option<bool> {
        if !self.search {
            return None;
        }
        Some(self.rows.get(&key).is_some_and(|r| ecad_electrical::devices::device_list::matches(&r.entry, query)))
    }
}

/// What the device feed says changed for the Devices table since change `since`: `All` when it
/// cannot say (rolled over) or when every row may differ (pages, settings). A location box is a
/// drawing aid: no row reads it (a device's location is its own).
pub fn changes(p: &ecad_engine::Project, since: Option<u64>) -> (Changed<DeviceId>, BTreeSet<DeviceId>) {
    let Some(t) = since.and_then(|n| p.device_changes_since(n)) else { return (Changed::All, BTreeSet::new()) };
    if t.pages || t.settings {
        return (Changed::All, BTreeSet::new());
    }
    if t.devices.is_empty() {
        return (Changed::Nothing, BTreeSet::new());
    }
    (Changed::Keys(t.devices.iter().copied().collect()), t.devices)
}
