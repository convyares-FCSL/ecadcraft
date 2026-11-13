//! Lists ▸ I/O (brief 045c, design 045 §8.4, Q14): a read-only column set over the I/O rows
//! (`ecad_cadcraft::plc::iolist`), grouped by card. Every cell is read; Place goes to the channel's
//! cell. Editing in the table is the PLC manager's (2.8).
//!
//! **Update map.** The rows come from the kept PLC model (`Outputs::plc`). After a commit only
//! the cards in `Plc::rows_changed` have their rows re-read and patched; a commit that touches no
//! PLC data patches nothing. Full rebuild only on first show or a reload of the models
//! (`build_id`).

use std::collections::{BTreeMap, HashMap};

use ecad_cadcraft::outputs::Outputs;
use ecad_cadcraft::plc::iolist::{self, Row};
use ecad_model::{ComponentId, Document};
use ecad_ui::table::{Cell, Changed, Column, ColumnId, Table, TableEvent, TableSource, Width};

use super::ListAction;

/// A row: its card and channel index.
pub type IoKey = (ComponentId, u32);

pub const CARD: ColumnId = ColumnId(0);
pub const RACK: ColumnId = ColumnId(1);
pub const SLOT: ColumnId = ColumnId(2);
pub const TYPE: ColumnId = ColumnId(3);
pub const CHANNEL: ColumnId = ColumnId(4);
pub const ADDRESS: ColumnId = ColumnId(5);
pub const PINS: ColumnId = ColumnId(6);
pub const TEXT: ColumnId = ColumnId(7);
pub const STATE: ColumnId = ColumnId(8);
pub const TARGETS: ColumnId = ColumnId(9);
pub const WIRE: ColumnId = ColumnId(10);
pub const PLACE: ColumnId = ColumnId(11);

/// The columns, every one read.
pub fn columns() -> Vec<Column> {
    vec![
        Column::read(CARD.0, "Card", Width::Initial(56.0)).groupable(),
        Column::read(RACK.0, "Rack", Width::Initial(56.0)).groupable(),
        Column::read(SLOT.0, "Slot", Width::Initial(40.0)),
        Column::read(TYPE.0, "Type", Width::Initial(40.0)).groupable(),
        Column::read(CHANNEL.0, "Ch", Width::Initial(32.0)),
        Column::read(ADDRESS.0, "Address", Width::Initial(64.0)),
        Column::read(PINS.0, "Pins", Width::Initial(56.0)),
        Column::read(TEXT.0, "Description", Width::Initial(160.0)),
        Column::read(STATE.0, "State", Width::Initial(64.0)).groupable(),
        Column::read(TARGETS.0, "Wired to", Width::Initial(120.0)),
        Column::read(WIRE.0, "Wire", Width::Initial(64.0)),
        Column::read(PLACE.0, "Place", Width::Remainder),
    ]
}

/// The I/O rows as a table source.
#[derive(Debug)]
pub struct IoSource {
    cols: Vec<Column>,
    keys: Vec<IoKey>,
    rows: HashMap<IoKey, Row>,
    by_card: BTreeMap<ComponentId, Vec<IoKey>>,
}

impl Default for IoSource {
    fn default() -> Self {
        Self { cols: columns(), keys: Vec::new(), rows: HashMap::new(), by_card: BTreeMap::new() }
    }
}

impl IoSource {
    /// Replace card `c`'s rows; the keys that changed.
    fn set_card(&mut self, c: ComponentId, rows: Vec<Row>) -> Vec<IoKey> {
        let mut touched: Vec<IoKey> = self.by_card.remove(&c).unwrap_or_default();
        for k in &touched {
            self.rows.remove(k);
        }
        let keys: Vec<IoKey> = rows.iter().map(|r| (c, r.index)).collect();
        for r in rows {
            self.rows.insert((c, r.index), r);
        }
        touched.extend(keys.iter().copied());
        if !keys.is_empty() {
            self.by_card.insert(c, keys);
        }
        touched
    }

    /// Lay the keys out in card order.
    fn order(&mut self, cards: &[ComponentId]) {
        self.keys = cards.iter().flat_map(|c| self.by_card.get(c).cloned().unwrap_or_default()).collect();
    }

    /// Row `k`.
    pub fn row(&self, k: IoKey) -> Option<&Row> {
        self.rows.get(&k)
    }
}

impl TableSource for IoSource {
    type Key = IoKey;

    fn columns(&self) -> &[Column] {
        &self.cols
    }

    fn keys(&self) -> &[IoKey] {
        &self.keys
    }

    fn contains(&self, key: IoKey) -> bool {
        self.rows.contains_key(&key)
    }

    fn cell(&self, key: IoKey, col: ColumnId) -> Cell {
        let Some(r) = self.rows.get(&key) else { return Cell::Empty };
        match col {
            CARD => Cell::read(r.card_tag.clone()),
            RACK => Cell::read(r.rack.clone().unwrap_or_default()),
            SLOT => Cell::read(r.slot.map(|s| s.to_string()).unwrap_or_default()),
            TYPE => Cell::read(r.ty),
            CHANNEL => Cell::read(r.channel.to_string()),
            ADDRESS => Cell::read(iolist::address_words(r)),
            PINS => Cell::read(iolist::pins_words(&r.pins)),
            TEXT => Cell::read(r.text.clone().unwrap_or_default()),
            STATE => Cell::read(r.state),
            TARGETS => Cell::read(iolist::targets_words(&r.targets)),
            WIRE => Cell::read(r.wire.clone().unwrap_or_default()),
            PLACE => Cell::Read { text: r.place.clone().unwrap_or_default(), go: r.page.is_some() },
            _ => Cell::Empty,
        }
    }

    fn row_label(&self, key: IoKey) -> String {
        self.rows.get(&key).map_or_else(String::new, |r| format!("Channel {} {}", r.card_tag, r.channel))
    }
}

/// Lists ▸ I/O: the source, the table and what it last read.
#[derive(Debug)]
pub struct IoView {
    pub source: IoSource,
    pub table: Table<IoKey>,
    /// (project serial, models build) of the last full build, and the models' revision read.
    built: Option<(u64, u64)>,
    /// The model's update count read (`Plc::updates`).
    revision: u64,
    /// Full rebuilds, and the cards patched in all (tests).
    pub rebuilds: u64,
    pub patched: u64,
}

impl Default for IoView {
    fn default() -> Self {
        let mut table = Table::new("ecad.iolist", "I/O");
        table.empty = "No PLC cards with channels yet.";
        Self { source: IoSource::default(), table, built: None, revision: 0, rebuilds: 0, patched: 0 }
    }
}

impl IoView {
    /// Forget what was read: the next sync rebuilds.
    pub fn invalidate(&mut self) {
        self.built = None;
    }

    /// Bring the view up to date with the models (the update map in the module docs).
    pub fn sync(&mut self, serial: u64, doc: &Document, o: &Outputs) {
        let cards: Vec<ComponentId> = iolist::cards_in_order(&o.plc).into_iter().map(|c| c.id).collect();
        if self.built != Some((serial, o.build_id)) {
            self.source = IoSource::default();
            for card in iolist::cards_in_order(&o.plc) {
                self.source.set_card(card.id, iolist::rows_of(doc, &o.plc, card));
            }
            self.source.order(&cards);
            self.built = Some((serial, o.build_id));
            self.revision = o.plc.updates;
            self.rebuilds += 1;
            self.table.update(&self.source, Changed::All);
            if self.table.view.group_by().is_none() && cards.len() > 1 {
                self.table.view.set_group(&self.source, Some(CARD));
            }
            return;
        }
        if o.plc.updates == self.revision {
            return;
        }
        // An update missed (the view was not shown): read everything again.
        if o.plc.updates != self.revision.wrapping_add(1) {
            self.built = None;
            return self.sync(serial, doc, o);
        }
        self.revision = o.plc.updates;
        let mut touched = Vec::new();
        for c in &o.plc.rows_changed {
            let rows = o.plc.cards.get(c).filter(|x| x.ty.has_channels()).map(|card| iolist::rows_of(doc, &o.plc, card)).unwrap_or_default();
            touched.extend(self.source.set_card(*c, rows));
            self.patched += 1;
        }
        // A card gone or no longer listed.
        let gone: Vec<ComponentId> = self.source.by_card.keys().copied().filter(|c| !cards.contains(c)).collect();
        for c in gone {
            touched.extend(self.source.set_card(c, Vec::new()));
        }
        if touched.is_empty() {
            return;
        }
        self.source.order(&cards);
        self.table.update(&self.source, Changed::Keys(touched));
    }

    /// Draw the table. Returns what it asks for (a go-to).
    pub fn show(&mut self, ui: &mut eframe::egui::Ui) -> Vec<ListAction> {
        let mut out = Vec::new();
        for e in self.table.show(ui, &self.source) {
            match e {
                TableEvent::GoTo { key, .. } | TableEvent::Open(key) => {
                    if let Some((pg, at)) = self.source.row(key).and_then(|r| r.page.zip(r.at)) {
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
