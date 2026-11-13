//! Devices as a column set (brief 039 step 3): tag and description set here (inline edits run
//! `setprop`, the same command, transaction and refusal as on the page); function, location
//! ("from box" where a box gives it), symbol, pages placed (a go-to), part and state read. Filter
//! chips: Provisional, Not on a page yet.
//!
//! **Update map.** The rows are the Devices list's entries ([`ecad_cadcraft::device_list`]) plus
//! each device's resolved location. On each sync the project's device feed
//! (`device_changes_since`) names the touched devices; only their rows are re-read and patched.
//! A touched location box (its outline or value) or changed project settings re-read every row's
//! location (rebuild); pages added, removed or renumbered change every place (rebuild). Every
//! other commit: nothing. Full rebuild only on first show, a new project, or a feed that rolled
//! over.

use std::collections::{BTreeSet, HashMap};

use ecad_cadcraft::device_list::DeviceEntry;
use ecad_model::ComponentId;
use ecad_ui::prim::Tone;
use ecad_ui::table::{Cell, Changed, Column, ColumnId, TableSource, Width};

pub const TAG: ColumnId = ColumnId(0);
pub const DESCRIPTION: ColumnId = ColumnId(1);
pub const FUNCTION: ColumnId = ColumnId(2);
pub const LOCATION: ColumnId = ColumnId(3);
pub const SYMBOL: ColumnId = ColumnId(4);
pub const PAGES: ColumnId = ColumnId(5);
pub const PART: ColumnId = ColumnId(6);
pub const STATE: ColumnId = ColumnId(7);

/// The chips.
pub const PROVISIONAL: &str = "Provisional";
pub const NOT_PLACED: &str = "Not on a page yet";

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

/// The columns.
pub fn columns() -> Vec<Column> {
    vec![
        Column::set(TAG.0, "Tag", Width::Initial(80.0)),
        Column::set(DESCRIPTION.0, "Description", Width::Initial(170.0)),
        Column::read(FUNCTION.0, "Function", Width::Initial(80.0)).groupable(),
        Column::read(LOCATION.0, "Location", Width::Initial(130.0)).groupable(),
        Column::read(SYMBOL.0, "Symbol", Width::Initial(150.0)).groupable(),
        Column::read(PAGES.0, "Placed", Width::Initial(110.0)),
        Column::read(PART.0, "Part", Width::Initial(120.0)).groupable(),
        Column::read(STATE.0, "State", Width::Remainder).groupable(),
    ]
}

/// One device's row: its list entry and its location as shown.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceRow {
    pub entry: DeviceEntry,
    /// `+CP1`, `+CP1 (from box on 105)`, `?`.
    pub location: String,
}

/// The devices as a table source, in the Devices list's order.
#[derive(Debug)]
pub struct DevicesSource {
    cols: Vec<Column>,
    keys: Vec<ComponentId>,
    rows: HashMap<ComponentId, DeviceRow>,
    /// The search matches as the Devices filter does (`k1` finds `-K01`).
    pub search: bool,
}

impl Default for DevicesSource {
    fn default() -> Self {
        Self { cols: columns(), keys: Vec::new(), rows: HashMap::new(), search: true }
    }
}

fn location(doc: &ecad_model::Document, c: ComponentId, cache: &mut ecad_cadcraft::locations::BoxCache) -> String {
    ecad_cadcraft::locations::resolve_with(doc, c, cache).shown(doc)
}

impl DevicesSource {
    pub fn row(&self, c: ComponentId) -> Option<&DeviceRow> {
        self.rows.get(&c)
    }

    /// Whether `c` is a row.
    pub fn contains_key(&self, c: ComponentId) -> bool {
        self.rows.contains_key(&c)
    }

    /// Read every row from `entries` (the whole list: first show, rebuild).
    pub fn build(&mut self, doc: &ecad_model::Document, entries: &[DeviceEntry]) {
        let mut cache = ecad_cadcraft::locations::BoxCache::default();
        self.keys = entries.iter().map(|e| e.id).collect();
        self.rows = entries.iter().map(|e| (e.id, DeviceRow { entry: e.clone(), location: location(doc, e.id, &mut cache) })).collect();
    }

    /// Re-read the rows of `touched` from `entries` (added, changed or removed), and the order.
    pub fn patch(&mut self, doc: &ecad_model::Document, entries: &[DeviceEntry], touched: &BTreeSet<ComponentId>) {
        let mut cache = ecad_cadcraft::locations::BoxCache::default();
        for c in touched {
            match entries.iter().find(|e| e.id == *c) {
                Some(e) => {
                    self.rows.insert(*c, DeviceRow { entry: e.clone(), location: location(doc, *c, &mut cache) });
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
    type Key = ComponentId;

    fn columns(&self) -> &[Column] {
        &self.cols
    }

    fn keys(&self) -> &[ComponentId] {
        &self.keys
    }

    fn contains(&self, key: ComponentId) -> bool {
        self.rows.contains_key(&key)
    }

    fn cell(&self, key: ComponentId, col: ColumnId) -> Cell {
        let Some(r) = self.rows.get(&key) else { return Cell::Empty };
        let e = &r.entry;
        let prop = |i: usize| e.properties.get(i).cloned().flatten().unwrap_or_default();
        match col {
            TAG => match e.tag.as_deref() {
                Some(t) if ecad_model::tag::is_provisional(t) => Cell::Provisional(t.to_string()),
                Some(t) => Cell::Input(t.to_string()),
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
                Cell::read(names.join(", "))
            }
            PAGES => Cell::Read { text: e.symbols.iter().map(|s| s.place.as_str()).collect::<Vec<_>>().join(", "), go: !e.symbols.is_empty() },
            PART => {
                let (maker, part) = (prop(2), prop(3));
                Cell::read(if maker.is_empty() || part.is_empty() { format!("{maker}{part}") } else { format!("{maker} {part}") })
            }
            STATE => {
                if e.tag.as_deref().is_some_and(ecad_model::tag::is_provisional) {
                    Cell::Badge(PROVISIONAL.into(), Tone::Provisional)
                } else if e.symbols.is_empty() {
                    Cell::Badge(NOT_PLACED.into(), Tone::Warning)
                } else if e.fixed {
                    Cell::Badge("Fixed".into(), Tone::Neutral)
                } else {
                    Cell::Empty
                }
            }
            _ => Cell::Empty,
        }
    }

    fn row_label(&self, key: ComponentId) -> String {
        self.rows.get(&key).map(|r| crate::devices_panel::device_label(&r.entry)).unwrap_or_default()
    }

    fn chips(&self) -> &[&'static str] {
        &[PROVISIONAL, NOT_PLACED]
    }

    fn chip(&self, key: ComponentId, chip: usize) -> bool {
        let Some(e) = self.rows.get(&key).map(|r| &r.entry) else { return false };
        match chip {
            0 => e.tag.as_deref().is_some_and(ecad_model::tag::is_provisional),
            1 => e.symbols.is_empty(),
            _ => true,
        }
    }

    fn matches(&self, key: ComponentId, query: &str) -> Option<bool> {
        if !self.search {
            return None;
        }
        Some(self.rows.get(&key).is_some_and(|r| ecad_cadcraft::device_list::matches(&r.entry, query)))
    }
}

/// What the device feed says changed for the Devices table since change `since`: `All` when it
/// cannot say (rolled over) or when every row may differ (pages, a location box, settings).
pub fn changes(p: &ecad_cadcraft::Project, since: Option<u64>) -> (Changed<ComponentId>, BTreeSet<ComponentId>) {
    let Some(t) = since.and_then(|n| p.device_changes_since(n)) else { return (Changed::All, BTreeSet::new()) };
    if t.pages || t.settings {
        return (Changed::All, BTreeSet::new());
    }
    let doc = p.ledger.document();
    // A location box changed: every device's location may differ (it is read from the boxes).
    if t.components.iter().any(|c| ecad_cadcraft::locations::is_box(doc, *c)) {
        return (Changed::All, BTreeSet::new());
    }
    if t.components.is_empty() {
        return (Changed::Nothing, BTreeSet::new());
    }
    (Changed::Keys(t.components.iter().copied().collect()), t.components)
}
