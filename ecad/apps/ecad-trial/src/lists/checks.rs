//! Checks as a column set (brief 039 step 2; design 022 §8's rows): severity, message, object,
//! page (a go-to) and the fix buttons.
//!
//! **Update map.** The source is a copy of the project's kept checks list (`update_checks` keeps
//! it from each transaction's ops) and the report models' checks. It is copied again only when
//! their revision moved; the copy is compared with the last one by each row's identity, and only
//! rows that appeared or went are patched in the table. Every other commit: nothing. Full rebuild
//! only on first show, a new project or a changed Show filter.

use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use ecad_cadcraft::checks::{Fix, Kind, Severity};
use ecad_ui::prim::Tone;
use ecad_ui::table::{Align, Cell, Changed, Column, ColumnId, TableSource, Width};

use crate::checks_panel::{self as cp, Row};

/// A row's identity: a hash of what the check says, and which of identical rows it is.
pub type CheckKey = (u64, u32);

pub const SEVERITY: ColumnId = ColumnId(0);
pub const MESSAGE: ColumnId = ColumnId(1);
pub const OBJECT: ColumnId = ColumnId(2);
pub const PAGE: ColumnId = ColumnId(3);
pub const FIX: ColumnId = ColumnId(4);

/// The columns.
pub fn columns() -> Vec<Column> {
    vec![
        Column::read(SEVERITY.0, "Severity", Width::Initial(72.0)).groupable(),
        Column::read(MESSAGE.0, "Message", Width::Remainder).groupable(),
        Column::read(OBJECT.0, "Object", Width::Initial(110.0)),
        Column::read(PAGE.0, "Page", Width::Initial(64.0)),
        Column::read(FIX.0, "Fix", Width::Initial(150.0)).unsorted().align(Align::Right),
    ]
}

/// Whether a check's row shows a count rather than a place (`12 wires`).
pub fn count_row(k: Kind) -> bool {
    matches!(
        k,
        Kind::Unnumbered
            | Kind::ProvisionalWires
            | Kind::NoPartNumber
            | Kind::NoLocation
            | Kind::NoCoreCount
            | Kind::NoAccessoryPart
            | Kind::Unracked
            | Kind::ItemNoPartNumber
            | Kind::FuseNoInsert
    )
}

/// The button labels of a fix.
pub fn fix_names(f: Fix) -> Vec<&'static str> {
    if let Some(two) = cp::pair(f) {
        return two.to_vec();
    }
    vec![match f {
        Fix::AddJunction(_) => cp::ADD_JUNCTION,
        Fix::Join(_) => cp::JOIN,
        Fix::NumberThem => cp::NUMBER_THEM,
        Fix::Finalise => cp::FINALISE,
        Fix::UseBoxLocation(_) => cp::USE_BOX_LOCATION,
        Fix::ShowReports | Fix::ShowCables => cp::SHOW,
        Fix::OpenStandards(_) => cp::GO_TO_STANDARDS,
        Fix::OfferDrawn(_) => cp::OFFER,
        Fix::RenumberStrip(_) => cp::RENUMBER_STRIP,
        Fix::SetCores(_) => cp::SET_CORES,
        Fix::NextFreeCore(_) => cp::NEXT_FREE_CORE,
        Fix::AccessoryOrphan(..) | Fix::LinkAcrossPartition(..) | Fix::LinkOrphan(..) | Fix::LinkOnNonStrip(_) => cp::REMOVE,
        Fix::MoveTable(_) => cp::MOVE_TABLE,
        Fix::RenumberPage(_) => cp::RENUMBER_PAGE,
        Fix::MoveToNextFree(_) => cp::MOVE_NEXT_FREE,
        Fix::MoveToNewPosition(_) => cp::MOVE_NEW_POSITION,
        Fix::LevelMissing(..) => cp::DROP_MISSING_LEVELS,
        Fix::FuseDevices => cp::MAKE_STRIP_POSITION,
        // Brief 045: the pairs are in `cp::pair`.
        Fix::Plc(_, ecad_cadcraft::plc::PlcFix::Unracked) => cp::PUT_IN_RACK,
        Fix::Plc(..) => cp::MOVE_TO_FREE_SLOT,
        // Brief 044.
        Fix::ConnectorNextPin(_) => cp::MOVE_NEXT_FREE,
        Fix::SetPins(..) => cp::SET_PINS,
        Fix::PlaceMates(..) => cp::PLACE_MATES,
        Fix::IgnoreConnector(..) => cp::IGNORE,
        Fix::UseArticle(..) => cp::USE_FOR_ALL,
        Fix::DropMissingFor(..) => cp::REMOVE,
    }]
}

/// The checks' rows as a table source.
#[derive(Debug)]
pub struct ChecksSource {
    cols: Vec<Column>,
    keys: Vec<CheckKey>,
    rows: HashMap<CheckKey, Row>,
    /// Fix buttons are offered (the project can be written).
    pub fixes: bool,
}

impl Default for ChecksSource {
    fn default() -> Self {
        Self { cols: columns(), keys: Vec::new(), rows: HashMap::new(), fixes: true }
    }
}

fn identity(r: &Row) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}|{:?}|{:?}|{:?}|{}|{:?}", r.check.kind, r.check.page, r.check.at, r.check.object, r.check.which, r.check.fix).hash(&mut h);
    r.locator.hash(&mut h);
    h.finish()
}

impl ChecksSource {
    /// The row of `key`.
    pub fn row(&self, key: CheckKey) -> Option<&Row> {
        self.rows.get(&key)
    }

    /// Replace the rows with `rows` (in book order). Returns what the table must do: the keys that
    /// appeared or went.
    pub fn set(&mut self, rows: &[Row]) -> Changed<CheckKey> {
        let mut seen: HashMap<u64, u32> = HashMap::new();
        let mut keys = Vec::with_capacity(rows.len());
        let mut map = HashMap::with_capacity(rows.len());
        for r in rows {
            let id = identity(r);
            let n = seen.entry(id).or_insert(0);
            let k = (id, *n);
            *n += 1;
            keys.push(k);
            map.insert(k, r.clone());
        }
        let old: BTreeSet<CheckKey> = self.keys.iter().copied().collect();
        let new: BTreeSet<CheckKey> = keys.iter().copied().collect();
        let touched: Vec<CheckKey> = old.symmetric_difference(&new).copied().collect();
        // A row whose place in the book order moved without changing (rare) is patched too.
        let moved = keys != self.keys && touched.is_empty();
        self.keys = keys;
        self.rows = map;
        if moved {
            Changed::All
        } else if touched.is_empty() {
            Changed::Nothing
        } else {
            Changed::Keys(touched)
        }
    }
}

impl TableSource for ChecksSource {
    type Key = CheckKey;

    fn columns(&self) -> &[Column] {
        &self.cols
    }

    fn keys(&self) -> &[CheckKey] {
        &self.keys
    }

    fn contains(&self, key: CheckKey) -> bool {
        self.rows.contains_key(&key)
    }

    fn cell(&self, key: CheckKey, col: ColumnId) -> Cell {
        let Some(r) = self.rows.get(&key) else { return Cell::Empty };
        let count = count_row(r.check.kind);
        match col {
            SEVERITY => match r.check.severity() {
                Severity::Error => Cell::Badge("Error".into(), Tone::Error),
                Severity::Warning => Cell::Badge("Warning".into(), Tone::Warning),
                Severity::Info => Cell::Badge("Info".into(), Tone::Neutral),
            },
            MESSAGE => Cell::Text(if count { r.check.which.clone() } else { r.check.what() }),
            OBJECT if count => Cell::Empty,
            OBJECT => Cell::read(r.check.which.clone()),
            PAGE => Cell::Read { text: r.locator.clone(), go: r.check.page.is_some() },
            FIX => match r.check.fix.filter(|_| self.fixes) {
                Some(f) => Cell::Buttons(fix_names(f).into_iter().map(str::to_string).collect()),
                None => Cell::Empty,
            },
            _ => Cell::Empty,
        }
    }

    fn row_label(&self, key: CheckKey) -> String {
        self.rows.get(&key).map(cp::row_label).unwrap_or_default()
    }

    fn chips(&self) -> &[&'static str] {
        &["Errors"]
    }

    fn chip(&self, key: CheckKey, _chip: usize) -> bool {
        self.rows.get(&key).is_some_and(|r| r.check.severity() == Severity::Error)
    }
}
