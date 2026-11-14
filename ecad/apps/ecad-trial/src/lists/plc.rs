//! Lists ▸ PLC (brief 051, design 051 §3; was 045c's Lists ▸ I/O): rack chips, the rack's tiles as
//! the table's group strip (screen m of the UI spec), and under them the I/O column set
//! (`ecad_cadcraft::plc::iolist`) filtered to the selected card (`This card`) or grouped by card
//! (`All cards`). Description, Address and Pins are inputs that run `plcchannel` (Q11); everything
//! else is read.
//!
//! **Update map.** The rows and tiles come from the kept PLC model (`Outputs::plc`). After a commit
//! only the cards in `Plc::rows_changed` have their rows re-read and patched, and only the tiles
//! of the cards in `Plc::last_changed` or `rows_changed` are made again (a rack among them, or a
//! change in the chips, makes the shown rack's tiles again). A page renumber changes only the link
//! words of the tiles whose pages moved (their numbers are compared per tile, no rows made). A
//! commit that touches no PLC data patches nothing. Full rebuild only on first show, a reload of
//! the models (`build_id`), or another rack chip.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ecad_cadcraft::outputs::Outputs;
use ecad_cadcraft::plc::iolist::{self, Row};
use ecad_cadcraft::plc::overview::{self, Chip, Square, Tile, TileKind};
use ecad_model::{ComponentId, Document, PageId};
use ecad_ui::table::strip::{self, Link, Mark, StripEvent, Tone};
use ecad_ui::table::{Cell, Changed, Column, ColumnId, Table, TableEvent, TableSource, Width};
use serde_json::json;

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

/// Element names (design 051 §3.8).
pub const STRIP: &str = "ecad.plc.strip";
pub const SCOPE: &str = "ecad.plc.table.scope";
/// The strip's line when the project has no PLC cards (design 051 §3.8).
pub const NO_CARDS: &str = "No PLC cards yet. Place a card body (Place \u{25b8} search \"card\").";
/// The tooltip of a greyed write link.
pub const WRITES_OFF: &str = "Writes are off.";
/// The scope switch's words.
pub const THIS_CARD: &str = "This card";
pub const ALL_CARDS: &str = "All cards";

/// The columns: every card (`all`), or one card (Slot, Rack, Card and Type hidden: the tile says
/// them). Description, Address and Pins are set here.
pub fn columns(all: bool) -> Vec<Column> {
    let mut v = Vec::new();
    if all {
        v.push(Column::read(CARD.0, "Card", Width::Initial(56.0)).groupable());
        v.push(Column::read(RACK.0, "Rack", Width::Initial(56.0)).groupable());
        v.push(Column::read(SLOT.0, "Slot", Width::Initial(40.0)));
        v.push(Column::read(TYPE.0, "Type", Width::Initial(40.0)).groupable());
    }
    v.extend([
        Column::read(CHANNEL.0, "Ch", Width::Initial(32.0)),
        Column::set(ADDRESS.0, "Address", Width::Initial(72.0)),
        Column::set(PINS.0, "Pins", Width::Initial(64.0)),
        Column::set(TEXT.0, "Description", Width::Initial(180.0)),
        Column::read(STATE.0, "State", Width::Initial(96.0)).groupable(),
        Column::read(TARGETS.0, "Wired to", Width::Initial(120.0)),
        Column::read(WIRE.0, "Wire", Width::Initial(64.0)),
        Column::read(PLACE.0, "Place", Width::Remainder),
    ]);
    v
}

/// The I/O rows as a table source.
#[derive(Debug)]
pub struct IoSource {
    cols: Vec<Column>,
    keys: Vec<IoKey>,
    rows: HashMap<IoKey, Row>,
    by_card: BTreeMap<ComponentId, Vec<IoKey>>,
    /// The card `This card` shows; `None`: every card.
    only: Option<ComponentId>,
    /// Inputs are offered (the project can be written).
    pub writable: bool,
}

impl Default for IoSource {
    fn default() -> Self {
        Self { cols: columns(true), keys: Vec::new(), rows: HashMap::new(), by_card: BTreeMap::new(), only: None, writable: false }
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

    /// Lay the keys out in card order (only the shown card's in `This card`).
    fn order(&mut self, cards: &[ComponentId]) {
        let shown: Vec<ComponentId> = match self.only {
            Some(c) => vec![c],
            None => cards.to_vec(),
        };
        self.keys = shown.iter().flat_map(|c| self.by_card.get(c).cloned().unwrap_or_default()).collect();
    }

    /// Show one card's rows (`Some`) or every card's.
    fn set_scope(&mut self, only: Option<ComponentId>) {
        self.only = only;
        self.cols = columns(only.is_none());
    }

    /// Row `k`.
    pub fn row(&self, k: IoKey) -> Option<&Row> {
        self.rows.get(&k)
    }

    /// The card `This card` shows.
    pub fn only(&self) -> Option<ComponentId> {
        self.only
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
        self.only.is_none_or(|c| c == key.0) && self.rows.contains_key(&key)
    }

    fn cell(&self, key: IoKey, col: ColumnId) -> Cell {
        let Some(r) = self.rows.get(&key) else { return Cell::Empty };
        let input = |t: String| if self.writable { Cell::Input(t) } else { Cell::read(t) };
        match col {
            CARD => Cell::read(r.card_tag.clone()),
            RACK => Cell::read(r.rack.clone().unwrap_or_default()),
            SLOT => Cell::read(r.slot.map(|s| s.to_string()).unwrap_or_default()),
            TYPE => Cell::read(r.ty),
            CHANNEL => Cell::read(r.channel.to_string()),
            ADDRESS => input(iolist::address_words(r)),
            PINS => input(iolist::pins_words(&r.pins)),
            TEXT => input(r.text.clone().unwrap_or_default()),
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

/// The `plcchannel` line an edit of `col` in row `r` writes (design 051 §3.7), or `None` when the
/// column is not set here or nothing would change.
pub fn edit_line(r: &Row, col: ColumnId, value: &str) -> Option<String> {
    let v = value.trim();
    let mut p = json!({"card": r.card.hex(), "channel": r.channel});
    match col {
        TEXT => p["text"] = if v.is_empty() { serde_json::Value::Null } else { json!(v) },
        ADDRESS => {
            // The `*` that marks a typed address is not part of it.
            let v = v.strip_suffix('*').unwrap_or(v).trim();
            p["address"] = if v.is_empty() { serde_json::Value::Null } else { json!(v) };
        }
        PINS => {
            let mut pins = serde_json::Map::new();
            match r.pins.as_slice() {
                [(role, _)] => {
                    pins.insert((*role).to_string(), json!(v));
                }
                many => {
                    // `+:1 -:5`: each part `role:pin`; a bare value is refused by the command.
                    for part in v.split_whitespace() {
                        let (role, pin) = part.split_once(':')?;
                        if !many.iter().any(|(r, _)| *r == role) {
                            return None;
                        }
                        pins.insert(role.to_string(), json!(pin));
                    }
                }
            }
            if pins.is_empty() {
                return None;
            }
            p["pins"] = serde_json::Value::Object(pins);
        }
        _ => return None,
    }
    Some(format!("plcchannel {p}"))
}

/// The mark of a square.
fn mark_of(s: Square) -> Mark {
    match s {
        Square::Used => Mark::On,
        Square::Spare => Mark::Off,
        Square::Reserved => Mark::Soft,
        Square::Check => Mark::Alert,
    }
}

/// The legend under the strip (design 051 §3.4).
pub const LEGEND: [(Mark, &str); 4] = [(Mark::On, "in use"), (Mark::Off, "free"), (Mark::Soft, "reserved"), (Mark::Alert, "has a check")];

/// `pages 106, 107`; more than three: `pages 106–112 (5)`.
fn pages_words(numbers: &[String]) -> Vec<String> {
    match numbers {
        [] => Vec::new(),
        [one] => vec![format!("page {one}")],
        many if many.len() <= 3 => {
            // `pages 106, 107`: each number its own link, the comma with the one before.
            let last = many.len() - 1;
            many.iter()
                .enumerate()
                .map(|(k, n)| {
                    let comma = if k < last { "," } else { "" };
                    if k == 0 { format!("pages {n}{comma}") } else { format!("{n}{comma}") }
                })
                .collect()
        }
        many => vec![format!(
            "pages {}\u{2013}{} ({})",
            many.first().map(String::as_str).unwrap_or_default(),
            many.last().map(String::as_str).unwrap_or_default(),
            many.len()
        )],
    }
}

/// What a tile's link part does.
#[derive(Debug, Clone, PartialEq)]
pub enum TileLink {
    Page(PageId),
    MakePage(ComponentId),
    Checks(ComponentId),
    Place { slot: u32 },
    None,
}

/// A tile as the strip draws it, and what its links do.
fn strip_tile(t: &Tile, numbers: &[String], writable: bool, doc: &Document) -> (strip::Tile, Vec<Vec<TileLink>>) {
    let name = format!("ecad.plc.tile.{}", t.element());
    let mut lines = Vec::new();
    let mut acts = Vec::new();
    let page_links = |out: &mut Vec<Link>, acts: &mut Vec<TileLink>| {
        let words = pages_words(numbers);
        let many = numbers.len() > 3;
        for (k, w) in words.into_iter().enumerate() {
            let p = if many { t.pages.first() } else { t.pages.get(k) };
            out.push(Link {
                text: w,
                tone: Tone::Accent,
                enabled: true,
                tip: None,
                name: format!("{name}.page.{}", numbers.get(k).cloned().unwrap_or_default()),
            });
            acts.push(p.map_or(TileLink::None, |p| TileLink::Page(*p)));
        }
    };
    match t.kind {
        TileKind::Free => {
            lines.push(vec![Link {
                text: "Place a card\u{2026}".into(),
                tone: Tone::Accent,
                enabled: writable,
                tip: (!writable).then(|| WRITES_OFF.into()),
                name: format!("{name}.place"),
            }]);
            acts.push(vec![TileLink::Place { slot: t.slot.unwrap_or(0) }]);
        }
        _ if !t.drawn => {
            lines.push(vec![Link { text: "not drawn yet".into(), tone: Tone::Warning, enabled: false, tip: None, name: String::new() }]);
            acts.push(vec![TileLink::None]);
            if let TileKind::Card(c) = t.kind {
                lines.push(vec![Link {
                    text: "Make its page".into(),
                    tone: Tone::Accent,
                    enabled: writable,
                    tip: (!writable).then(|| WRITES_OFF.into()),
                    name: format!("{name}.makepage"),
                }]);
                acts.push(vec![TileLink::MakePage(c)]);
            }
        }
        _ => {
            let mut parts = Vec::new();
            let mut a = Vec::new();
            // UI rev 3 (board 2.03): how many channels are in use, then the page link.
            if !t.squares.is_empty() {
                let used = t.squares.iter().filter(|s| **s != Square::Spare).count();
                parts.push(Link { text: format!("{used} of {} in use", t.squares.len()), tone: Tone::Plain, ..Link::default() });
                a.push(TileLink::None);
            }
            page_links(&mut parts, &mut a);
            if t.checks > 0 {
                let words = if t.checks == 1 { "\u{b7} 1 check".to_string() } else { format!("\u{b7} {} checks", t.checks) };
                parts.push(Link { text: words, tone: Tone::Error, enabled: true, tip: None, name: format!("{name}.checks") });
                a.push(match t.kind {
                    TileKind::Card(c) | TileKind::Rack(c) => TileLink::Checks(c),
                    TileKind::Free => TileLink::None,
                });
            }
            lines.push(parts);
            acts.push(a);
        }
    }
    // UI rev 3 (board 2.03): what the card is at the head's right, its addresses under the squares.
    let (kind, info, under) = match t.kind {
        TileKind::Free => (String::new(), vec![t.what.clone()], String::new()),
        _ if t.squares.is_empty() => (String::new(), vec![t.what.clone(), t.range.clone()], String::new()),
        _ => (t.what.clone(), Vec::new(), t.range.clone()),
    };
    let tips = match t.card() {
        Some(c) => (0..t.squares.len()).map(|i| tip_words(doc, c, t, i)).collect(),
        None => Vec::new(),
    };
    let tile = strip::Tile {
        name: name.clone(),
        head: t.head.clone(),
        kind,
        under,
        title: t.tag.clone(),
        faint_title: t.kind == TileKind::Free,
        lines: info,
        marks: t.squares.iter().map(|s| mark_of(*s)).collect(),
        mark_name: format!("{name}.ch."),
        tips,
        links: lines,
        dashed: t.kind == TileKind::Free,
    };
    (tile, acts)
}

/// A square's hover (045 §4.7's words): `-A03 channel 3 · Q0.3 · Pump 1 run`.
fn tip_words(doc: &Document, c: ComponentId, t: &Tile, i: usize) -> String {
    let n = u32::try_from(i).unwrap_or(0);
    let first = ecad_cadcraft::plc::channels::first_number(doc, c);
    let state = t.squares.get(i).map_or("", |s| s.words());
    format!("{} channel {} \u{b7} {state}", t.tag, n.saturating_add(first))
}

/// Lists ▸ PLC: the chips, the strip, the source, the table and what it last read.
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
    /// The rack chips, and the one shown.
    pub chips: Vec<Chip>,
    pub chip: usize,
    /// The shown chip's tiles, as read and as the strip draws them, and what their links do.
    pub tiles: Vec<Tile>,
    pub strip: Vec<strip::Tile>,
    /// Each tile's symbols, first page first: selecting a tile selects them, so the inspector
    /// shows the card (overseer's look note on #89).
    pub symbols: Vec<Vec<ecad_model::RepresentationId>>,
    links: Vec<Vec<Vec<TileLink>>>,
    /// Each tile's page numbers as last shown.
    numbers: Vec<Vec<String>>,
    /// `n channels, m in use`.
    pub totals: String,
    /// The selected tile.
    pub selected: Option<usize>,
    /// `All cards` (else `This card`).
    pub all_cards: bool,
    /// Tile builds of the whole strip, and single tiles made again (tests: the update map).
    pub strip_builds: u64,
    pub tiles_patched: u64,
    /// The cell whose edit is running, for its refusal.
    pending: Option<(IoKey, ColumnId)>,
    /// A row to select once the table shows its card.
    pending_reveal: Option<IoKey>,
}

impl Default for IoView {
    fn default() -> Self {
        let mut table = Table::new("ecad.iolist", "I/O");
        table.empty = "No PLC cards with channels yet.";
        Self {
            source: IoSource::default(),
            table,
            built: None,
            revision: 0,
            rebuilds: 0,
            patched: 0,
            chips: Vec::new(),
            chip: 0,
            tiles: Vec::new(),
            strip: Vec::new(),
            symbols: Vec::new(),
            links: Vec::new(),
            numbers: Vec::new(),
            totals: String::new(),
            selected: None,
            all_cards: false,
            strip_builds: 0,
            tiles_patched: 0,
            pending: None,
            pending_reveal: None,
        }
    }
}

impl IoView {
    /// Forget what was read: the next sync rebuilds.
    pub fn invalidate(&mut self) {
        self.built = None;
    }

    /// The selected card, if the selected tile is one.
    pub fn selected_card(&self) -> Option<ComponentId> {
        self.selected.and_then(|i| self.tiles.get(i)).and_then(Tile::card)
    }

    /// The page numbers of a tile's pages.
    fn numbers_of(doc: &Document, t: &Tile) -> Vec<String> {
        t.pages.iter().map(|p| ecad_cadcraft::references::page_number(doc, *p)).collect()
    }

    /// Make the shown chip's tiles again.
    fn build_strip(&mut self, doc: &Document, o: &Outputs) {
        let plc = &o.plc;
        let now = overview::chips(plc);
        let keep = self.chips.get(self.chip).cloned();
        self.chips = now;
        self.chip = keep.and_then(|k| self.chips.iter().position(|c| same_chip(c, &k))).unwrap_or(0);
        let card = self.selected_card();
        let (tiles, totals) = self.chips.get(self.chip).map_or_else(|| (Vec::new(), (0, 0)), |c| overview::chip_tiles(doc, plc, c));
        self.totals = overview::totals_words(totals);
        self.numbers = tiles.iter().map(|t| Self::numbers_of(doc, t)).collect();
        let made: Vec<_> = tiles.iter().zip(&self.numbers).map(|(t, n)| strip_tile(t, n, self.source.writable, doc)).collect();
        self.strip = made.iter().map(|(t, _)| t.clone()).collect();
        self.links = made.into_iter().map(|(_, l)| l).collect();
        self.symbols = tiles.iter().map(|t| symbols_of(doc, t)).collect();
        self.tiles = tiles;
        // Keep the selected card when it is still shown, else the first card tile.
        self.selected =
            card.and_then(|c| self.tiles.iter().position(|t| t.card() == Some(c))).or_else(|| self.tiles.iter().position(|t| t.card().is_some()));
        self.strip_builds += 1;
    }

    /// Make tile `i` again from the model.
    fn patch_tile(&mut self, doc: &Document, o: &Outputs, i: usize) {
        let Some(c) = self.tiles.get(i).and_then(Tile::card) else { return };
        let Some(card) = o.plc.cards.get(&c) else { return };
        let head = self.tiles.get(i).map(|t| t.head.clone()).unwrap_or_default();
        let t = overview::card_tile(doc, &o.plc, card, head);
        let numbers = Self::numbers_of(doc, &t);
        let (st, links) = strip_tile(&t, &numbers, self.source.writable, doc);
        if let Some(s) = self.symbols.get_mut(i) {
            *s = symbols_of(doc, &t);
        }
        if let (Some(a), Some(b), Some(c2), Some(d)) = (self.tiles.get_mut(i), self.strip.get_mut(i), self.links.get_mut(i), self.numbers.get_mut(i))
        {
            *a = t;
            *b = st;
            *c2 = links;
            *d = numbers;
        }
        self.tiles_patched += 1;
    }

    /// The table's scope from the selection: the selected card, or every card.
    fn apply_scope(&mut self, cards: &[ComponentId]) {
        let only = if self.all_cards { None } else { self.selected_card() };
        if only != self.source.only() || self.source.columns().len() != columns(only.is_none()).len() {
            self.source.set_scope(only);
            self.source.order(cards);
            self.table.reset();
            self.table.update(&self.source, Changed::All);
            if only.is_none() && self.table.view.group_by().is_none() && cards.len() > 1 {
                self.table.view.set_group(&self.source, Some(CARD));
            }
        }
        if let Some(k) = self.pending_reveal.take() {
            self.table.reveal(&[k]);
        }
    }

    /// Bring the view up to date with the models (the update map in the module docs). `writable`:
    /// the project can be written now.
    pub fn sync(&mut self, serial: u64, doc: &Document, o: &Outputs, writable: bool) {
        let cards: Vec<ComponentId> = iolist::cards_in_order(&o.plc).into_iter().map(|c| c.id).collect();
        if self.source.writable != writable {
            self.source.writable = writable;
            self.built = None;
        }
        if self.built != Some((serial, o.build_id)) {
            let only = self.source.only();
            self.source = IoSource { writable, ..IoSource::default() };
            for card in iolist::cards_in_order(&o.plc) {
                self.source.set_card(card.id, iolist::rows_of(doc, &o.plc, card));
            }
            self.build_strip(doc, o);
            self.source.set_scope(only);
            self.source.order(&cards);
            self.built = Some((serial, o.build_id));
            self.revision = o.plc.updates;
            self.rebuilds += 1;
            self.table.reset();
            self.table.update(&self.source, Changed::All);
            self.apply_scope(&cards);
            return;
        }
        if o.plc.updates == self.revision {
            self.renumbered(doc);
            self.apply_scope(&cards);
            return;
        }
        // An update missed (the view was not shown): read everything again.
        if o.plc.updates != self.revision.wrapping_add(1) {
            self.built = None;
            return self.sync(serial, doc, o, writable);
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
        // The strip: the tiles of the changed cards; a changed rack or chip set makes it again.
        let changed: BTreeSet<ComponentId> = o.plc.last_changed.iter().chain(o.plc.rows_changed.iter()).copied().collect();
        if !changed.is_empty() {
            let rack_changed = changed.iter().any(|c| o.plc.racks.contains_key(c) || !o.plc.cards.contains_key(c));
            let chips_moved = overview::chips(&o.plc) != self.chips;
            let shown: Vec<usize> =
                self.tiles.iter().enumerate().filter(|(_, t)| t.card().is_some_and(|c| changed.contains(&c))).map(|(i, _)| i).collect();
            // A card that moved in or out of the shown rack changes the layout.
            let moved = changed.iter().any(|c| {
                let here = self.tiles.iter().any(|t| t.card() == Some(*c));
                let belongs = o.plc.cards.get(c).is_some_and(|card| match (self.chips.get(self.chip), card.rack) {
                    (Some(Chip::Rack(r, _)), ecad_cadcraft::plc::rack::RackRef::Rack(x)) => *r == x,
                    (Some(Chip::NoRack(_)), ecad_cadcraft::plc::rack::RackRef::Rack(_)) => false,
                    (Some(Chip::NoRack(_)), _) => true,
                    _ => false,
                });
                here != belongs || (here && o.plc.cards.get(c).map(|x| x.slot) != self.tiles.iter().find(|t| t.card() == Some(*c)).map(|t| t.slot))
            });
            if rack_changed || chips_moved || moved {
                self.build_strip(doc, o);
            } else {
                for i in shown {
                    self.patch_tile(doc, o, i);
                }
                let totals = self.chips.get(self.chip).map(|ch| overview::chip_totals(&o.plc, ch)).unwrap_or((0, 0));
                self.totals = overview::totals_words(totals);
            }
        }
        if !touched.is_empty() {
            self.source.order(&cards);
            self.table.update(&self.source, Changed::Keys(touched));
        }
        self.apply_scope(&cards);
    }

    /// A page renumbered: the link words of the tiles whose numbers moved.
    fn renumbered(&mut self, doc: &Document) {
        for i in 0..self.tiles.len() {
            let Some(t) = self.tiles.get(i) else { continue };
            let now = Self::numbers_of(doc, t);
            if self.numbers.get(i) != Some(&now) {
                let (st, links) = strip_tile(t, &now, self.source.writable, doc);
                if let (Some(b), Some(c), Some(d)) = (self.strip.get_mut(i), self.links.get_mut(i), self.numbers.get_mut(i)) {
                    *b = st;
                    *c = links;
                    *d = now;
                }
                self.tiles_patched += 1;
            }
        }
    }

    /// Show rack chip `i`.
    pub fn choose_chip(&mut self, i: usize) {
        if i != self.chip && i < self.chips.len() {
            self.chip = i;
            self.selected = None;
            self.built = None;
        }
    }

    /// Select tile `i` (filters the table to its card).
    pub fn select(&mut self, i: usize) {
        if i < self.tiles.len() {
            self.selected = Some(i);
        }
    }

    /// The result of the last edit's command: a refusal shows under its cell.
    pub fn ran(&mut self, refusal: Option<String>) {
        if let (Some((k, col)), Some(msg)) = (self.pending.take(), refusal) {
            self.table.refuse(k, col, msg);
        }
    }

    /// Draw the chips, the strip, the legend and the table. Returns what it asks for.
    pub fn show(&mut self, ui: &mut eframe::egui::Ui) -> Vec<ListAction> {
        use eframe::egui;
        let mut out = Vec::new();
        let s = ecad_ui::SHELL;
        let ty = ecad_ui::tokens::ui_type::BODY;
        // UI rev 3 (board 2.03): the rack's line (its chips, its totals) and the legend.
        ui.add_space(ecad_ui::tokens::space::M);
        ui.horizontal(|ui| {
            ui.add_space(ecad_ui::tokens::space::M);
            if self.chips.len() > 1 {
                let words: Vec<String> = self.chips.iter().map(Chip::words).collect();
                let labels: Vec<&str> = words.iter().map(String::as_str).collect();
                let mut i = self.chip;
                ecad_ui::prim::segmented(ui, &mut i, &labels);
                if i != self.chip {
                    out.push(ListAction::Chip(i));
                }
            } else if let Some(c) = self.chips.first() {
                let title = ecad_ui::fonts::font(ui.ctx(), ecad_ui::tokens::ui_type::TITLE);
                ui.label(egui::RichText::new(c.words()).font(title).color(ecad_ui::color(s.text)));
            }
            ui.label(egui::RichText::new(&self.totals).size(ty.size).color(ecad_ui::color(s.text_2)));
            if !self.tiles.is_empty() {
                ui.add_space(ecad_ui::tokens::space::XXL);
                strip::legend(ui, &LEGEND);
            }
        });
        ui.add_space(ecad_ui::tokens::space::S);
        if self.tiles.is_empty() {
            ui.label(egui::RichText::new(NO_CARDS).color(ecad_ui::color(s.text_3)));
        } else {
            let events = strip::show(ui, egui::Id::unique(STRIP), &self.strip, self.selected);
            for e in events {
                match e {
                    StripEvent::Select(i) => out.push(ListAction::Tile(i)),
                    StripEvent::Mark { tile, mark, double } => {
                        out.push(ListAction::Tile(tile));
                        let Some(c) = self.tiles.get(tile).and_then(Tile::card) else { continue };
                        let key = (c, u32::try_from(mark).unwrap_or(0));
                        if double {
                            match self.source.row(key).and_then(|r| r.page.zip(r.at)) {
                                Some((pg, at)) => out.push(ListAction::GoTo(pg, at)),
                                None => {
                                    let n = self.source.row(key).map_or(mark as u32, |r| r.channel);
                                    out.push(ListAction::Say(format!("Channel {n} is not drawn yet.")));
                                }
                            }
                        } else {
                            out.push(ListAction::Reveal(key));
                        }
                    }
                    StripEvent::Link { tile, line, part } => match self.links.get(tile).and_then(|l| l.get(line)).and_then(|l| l.get(part)) {
                        Some(TileLink::Page(p)) => out.push(ListAction::GoToCard(*p, self.tiles.get(tile).and_then(Tile::card))),
                        Some(TileLink::MakePage(c)) => out.push(ListAction::Run(format!("plcpage {}", json!({"card": c.hex()})))),
                        Some(TileLink::Checks(c)) => out.push(ListAction::ChecksOf(*c)),
                        Some(TileLink::Place { slot }) => out.push(ListAction::PlaceCard(*slot)),
                        Some(TileLink::None) | None => {}
                    },
                }
            }
        }
        ui.add_space(ecad_ui::tokens::space::L);
        // The selected card's line (board 2.03), then the scope switch over the table.
        ui.horizontal(|ui| {
            ui.add_space(ecad_ui::tokens::space::M);
            if let Some(t) = self.selected.and_then(|i| self.tiles.get(i)) {
                let words = format!("{} \u{b7} {} \u{b7} {}", t.tag, t.what, t.head);
                let strong = ecad_ui::fonts::font(ui.ctx(), ecad_ui::tokens::ui_type::BODY_EMPHASIS);
                ui.label(egui::RichText::new(words).font(strong).color(ecad_ui::color(s.text)));
                let used = t.squares.iter().filter(|q| **q != Square::Spare).count();
                let n = t.squares.len();
                let place = t.place.as_deref().map_or_else(|| "on no page".to_string(), |p| format!("card at {p}"));
                ui.label(egui::RichText::new(format!("{n} channels, {used} in use, {place}")).size(ty.size).color(ecad_ui::color(s.text_2)));
                ui.add_space(ecad_ui::tokens::space::L);
            }
            let mut i = usize::from(self.all_cards);
            ui.push_id(SCOPE, |ui| ecad_ui::prim::segmented(ui, &mut i, &[THIS_CARD, ALL_CARDS]));
            if (i == 1) != self.all_cards {
                out.push(ListAction::Scope(i == 1));
            }
        });
        for e in self.table.show(ui, &self.source) {
            match e {
                TableEvent::Commit { key, col, value } if self.source.writable => {
                    if let Some(line) = self.source.row(key).and_then(|r| edit_line(r, col, &value)) {
                        self.pending = Some((key, col));
                        out.push(ListAction::Run(line));
                    }
                }
                TableEvent::GoTo { key, .. } | TableEvent::Open(key) => {
                    if let Some((pg, at)) = self.source.row(key).and_then(|r| r.page.zip(r.at)) {
                        out.push(ListAction::GoTo(pg, at));
                    }
                }
                TableEvent::Selected(keys) if self.all_cards => {
                    if let Some(c) = keys.first().map(|k| k.0)
                        && let Some(i) = self.tiles.iter().position(|t| t.card() == Some(c))
                    {
                        self.selected = Some(i);
                    }
                }
                TableEvent::Export(csv) => super::copy_csv(ui, csv),
                TableEvent::Commit { .. } | TableEvent::Selected(_) | TableEvent::Button { .. } => {}
            }
        }
        out
    }

    /// Act on what [`IoView::show`] asked that the view itself answers (tile, chip, scope, reveal);
    /// the rest goes back to the trial.
    pub fn handle(&mut self, a: ListAction) -> Option<ListAction> {
        match a {
            ListAction::Tile(i) => {
                let changed = self.selected != Some(i);
                self.select(i);
                // The card's symbols selected on its first page: the inspector shows the card.
                let reps = self.symbols.get(i).cloned().unwrap_or_default();
                (changed && !reps.is_empty()).then_some(ListAction::Panel(crate::project_panel::Action::GotoSymbols(reps)))
            }
            ListAction::Chip(i) => {
                self.choose_chip(i);
                None
            }
            ListAction::Scope(all) => {
                self.all_cards = all;
                None
            }
            ListAction::Reveal(key) => {
                self.pending_reveal = Some(key);
                None
            }
            other => Some(other),
        }
    }
}

/// The symbols of a tile's card or rack, by page in book order (at most 256).
fn symbols_of(doc: &Document, t: &Tile) -> Vec<ecad_model::RepresentationId> {
    let c = match t.kind {
        TileKind::Card(c) | TileKind::Rack(c) => c,
        TileKind::Free => return Vec::new(),
    };
    let mut out = Vec::new();
    for page in &t.pages {
        out.extend(doc.representations_of(ecad_model::Subject::Component(c)).filter(|(_, r)| r.page == *page).map(|(id, _)| id));
        if out.len() >= 256 {
            out.truncate(256);
            break;
        }
    }
    out
}

fn same_chip(a: &Chip, b: &Chip) -> bool {
    match (a, b) {
        (Chip::Rack(x, _), Chip::Rack(y, _)) => x == y,
        (Chip::NoRack(_), Chip::NoRack(_)) => true,
        _ => false,
    }
}
