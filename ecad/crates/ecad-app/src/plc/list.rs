//! Lists ▸ PLC (brief 051, design 051 §3; was 045c's Lists ▸ I/O): rack chips, the rack's tiles as
//! the table's group strip (screen m of the UI spec), and under them the I/O column set
//! (`ecad_engine::plc::iolist`) filtered to the selected card (`This card`) or grouped by card
//! (`All cards`). Description, Address and Pins are inputs that run `plcchannel` (Q11); everything
//! else is read.
//!
//! **Update map.** The rows and tiles come from the kept PLC model (`ReportModels::plc`). After a commit
//! only the cards in `Plc::rows_changed` have their rows re-read and patched, and only the tiles
//! of the cards in `Plc::last_changed` or `rows_changed` are made again (a rack among them, or a
//! change in the chips, makes the shown rack's tiles again). A page renumber changes only the link
//! words of the tiles whose pages moved (their numbers are compared per tile, no rows made). A
//! commit that touches no PLC data patches nothing. Full rebuild only on first show, a reload of
//! the models (`build_id`), or another rack chip.
//!
//! Brief 056 (design 056 §3, §4, board 2.03): the rack heading line, the tiles' kind words, the
//! card header with the PLC chip, **Compare**, **Export tags** and **→ Page**, the signal columns
//! `Ch, Address, Tag, Description, Type, Safe, Drives, Page, PLC, Test`, free channels folded
//! after the first of a run, and Import tags with its preview ([`super::plc_compare`]). Each row
//! carries its sync state from the kept index (`Plc::sync`); the same update map patches it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ecad_engine::outputs::kept::ReportModels;
use ecad_engine::plc::iolist::{self, Row};
use ecad_engine::plc::overview::{self, Chip, Square, Tile, TileKind};
use ecad_engine::plc::signals;
use ecad_model::{DeviceId, Document, PageId};
use ecad_ui::table::strip::{self, Link, Mark, StripEvent, Tone};
use ecad_ui::table::{Cell, Changed, Column, ColumnId, Table, TableEvent, TableSource, Width};
use serde_json::json;

use super::ListAction;

/// A row: its card and channel index.
pub type IoKey = (DeviceId, u32);

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
/// Brief 056 (design 056 §4.2).
pub const TAG: ColumnId = ColumnId(12);
pub const SAFE: ColumnId = ColumnId(13);
pub const DRIVES: ColumnId = ColumnId(14);
pub const PAGE: ColumnId = ColumnId(15);
pub const TEST: ColumnId = ColumnId(17);

/// Element names (design 056 §3.1, §4.1).
pub const CARD_HEAD: &str = "ecad.plc.cardhead";
pub const EXPORT_TAGS: &str = "ecad.plc.exporttags";
pub const GO_PAGE: &str = "ecad.plc.gopage";
pub const MORE_FREE: &str = "ecad.plc.morefree";
pub const ADD_RACK: &str = "ecad.plc.addrack";
pub const MAKE_TAGS: &str = "ecad.plc.maketags";
/// A table row's height, px (039's table: a 30 px row; the header is one row too).
const TABLE_ROW_PX: f32 = 30.0;
/// The free channel's Tag placeholder.
pub const FREE: &str = "free";
/// The Drives header's words for inputs (design 056 §4.5) and the Test column's (§4.6).
pub const DRIVES_TIP: &str = "For outputs, what the channel switches. For inputs, what feeds it.";
pub const TESTS_COME: &str = "Tests come with the Tests tab.";

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

/// The columns (design 056 §4.2, without the PLC column since the owner's change of 2026-11-14):
/// one card's signal columns `Ch, Address, Tag, Description, Type, Safe, Drives, Page, Test`;
/// every card (`all`) keeps 051's columns plus Tag and Safe. Address, Tag, Description, Safe and
/// Pins are set here.
pub fn columns(all: bool) -> Vec<Column> {
    if all {
        return vec![
            Column::read(CARD.0, "Card", Width::Initial(56.0)).groupable(),
            Column::read(RACK.0, "Rack", Width::Initial(56.0)).groupable(),
            Column::read(SLOT.0, "Slot", Width::Initial(40.0)),
            Column::read(TYPE.0, "Type", Width::Initial(44.0)).groupable(),
            Column::read(CHANNEL.0, "Ch", Width::Initial(32.0)),
            Column::set(ADDRESS.0, "Address", Width::Initial(80.0)),
            Column::set(TAG.0, "Tag", Width::Initial(140.0)),
            Column::set(TEXT.0, "Description", Width::Initial(180.0)),
            Column::set(PINS.0, "Pins", Width::Initial(64.0)),
            Column::read(STATE.0, "State", Width::Initial(80.0)).groupable(),
            Column::set(SAFE.0, "Safe", Width::Initial(60.0)),
            Column::read(TARGETS.0, "Wired to", Width::Initial(120.0)),
            Column::read(WIRE.0, "Wire", Width::Initial(64.0)),
            Column::read(PLACE.0, "Place", Width::Remainder),
        ];
    }
    vec![
        Column::read(CHANNEL.0, "Ch", Width::Initial(32.0)),
        Column::set(ADDRESS.0, "Address", Width::Initial(80.0)),
        Column::set(TAG.0, "Tag", Width::Initial(140.0)),
        Column::set(TEXT.0, "Description", Width::Initial(200.0)),
        Column::read(TYPE.0, "Type", Width::Initial(44.0)),
        Column::set(SAFE.0, "Safe", Width::Initial(60.0)),
        Column::read(DRIVES.0, "Drives", Width::Initial(120.0)),
        Column::read(PAGE.0, "Page", Width::Initial(56.0)),
        Column::read(TEST.0, "Test", Width::Remainder),
    ]
}

/// The I/O rows as a table source.
#[derive(Debug)]
pub struct IoSource {
    cols: Vec<Column>,
    keys: Vec<IoKey>,
    rows: HashMap<IoKey, Row>,
    by_card: BTreeMap<DeviceId, Vec<IoKey>>,
    /// The card `This card` shows; `None`: every card.
    only: Option<DeviceId>,
    /// Inputs are offered (the project can be written).
    pub writable: bool,
    /// Free channels folded away after the first of a run (`This card` only), and whether the
    /// user unfolded them.
    pub folded: usize,
    pub unfold: bool,
    shown: std::collections::HashSet<IoKey>,
}

impl Default for IoSource {
    fn default() -> Self {
        Self {
            cols: columns(true),
            keys: Vec::new(),
            rows: HashMap::new(),
            by_card: BTreeMap::new(),
            only: None,
            writable: false,
            folded: 0,
            unfold: false,
            shown: std::collections::HashSet::new(),
        }
    }
}

impl IoSource {
    /// Replace card `c`'s rows; the keys that changed.
    fn set_card(&mut self, c: DeviceId, rows: Vec<Row>) -> Vec<IoKey> {
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

    /// Lay the keys out in card order (only the shown card's in `This card`, its free channels
    /// folded after the first of each run unless unfolded; design 056 §4.2).
    fn order(&mut self, cards: &[DeviceId]) {
        let shown: Vec<DeviceId> = match self.only {
            Some(c) => vec![c],
            None => cards.to_vec(),
        };
        let all: Vec<IoKey> = shown.iter().flat_map(|c| self.by_card.get(c).cloned().unwrap_or_default()).collect();
        self.folded = 0;
        if self.only.is_some() && !self.unfold {
            let mut keys = Vec::with_capacity(all.len());
            let mut run = false;
            for k in all {
                let free = self.is_free(k);
                if free && run {
                    self.folded += 1;
                    continue;
                }
                run = free;
                keys.push(k);
            }
            self.keys = keys;
        } else {
            self.keys = all;
        }
        self.shown = self.keys.iter().copied().collect();
    }

    /// A free channel (design 056 §4.2): no tag, no description, not wired.
    pub fn is_free(&self, k: IoKey) -> bool {
        self.rows.get(&k).is_some_and(Row::is_free)
    }

    /// Show one card's rows (`Some`) or every card's.
    fn set_scope(&mut self, only: Option<DeviceId>) {
        self.only = only;
        self.cols = columns(only.is_none());
    }

    /// Row `k`.
    pub fn row(&self, k: IoKey) -> Option<&Row> {
        self.rows.get(&k)
    }

    /// The card `This card` shows.
    pub fn only(&self) -> Option<DeviceId> {
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
        self.only.is_none_or(|c| c == key.0) && self.rows.contains_key(&key) && self.shown.contains(&key)
    }

    fn cell(&self, key: IoKey, col: ColumnId) -> Cell {
        let Some(r) = self.rows.get(&key) else { return Cell::Empty };
        let input = |t: String| if self.writable { Cell::Input(t) } else { Cell::read(t) };
        // Brief 056 (design 056 §4.2): a free channel shows Ch, Address, `free` in Tag, and an
        // empty Description (typing either reserves it).
        if self.only.is_some() && self.is_free(key) && !matches!(col, CHANNEL | ADDRESS | TAG | TEXT) {
            return Cell::Empty;
        }
        match col {
            TAG if r.tag.is_none() && self.is_free(key) => input(FREE.into()),
            TAG => input(r.tag.clone().unwrap_or_default()),
            SAFE => match r.safe_shown() {
                Some((s, _)) => input(signals::safe_words(s)),
                None => Cell::read("\u{2014}"),
            },
            TYPE if self.only.is_some() => Cell::read(r.ty),
            DRIVES => {
                let (d, _) = signals::drives(&r.targets);
                let go = d.first().is_some_and(|t| t.page.is_some() && t.at.is_some());
                Cell::Read { text: signals::drives_words(&r.targets), go }
            }
            PAGE => Cell::Read { text: r.place.clone().unwrap_or_else(|| "none".into()), go: r.page.is_some() },
            TEST => Cell::read(r.test.words()),
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
        // Brief 056: the placeholder of a free channel is not a tag.
        TAG if r.tag.is_none() && (v == FREE || v.is_empty()) => return None,
        TAG => p["tag"] = if v.is_empty() { serde_json::Value::Null } else { json!(v) },
        SAFE => {
            // Inputs have no safe state.
            r.safe_default.as_ref()?;
            // The own value typed again changes nothing; the default's value typed sets it here.
            if r.safe.as_deref().is_some_and(|s| signals::safe_words(s) == v) {
                return None;
            }
            p["safe"] = if v.is_empty() { serde_json::Value::Null } else { json!(v) };
        }
        ADDRESS => {
            // The `*` that marks a typed address is not part of it.
            let v = v.strip_suffix('*').unwrap_or(v).trim();
            p["address"] = if v.is_empty() { serde_json::Value::Null } else { json!(v) };
        }
        PINS => {
            let mut pins = serde_json::Map::new();
            match r.pins.as_slice() {
                [(role, _)] => {
                    pins.insert(ecad_model::Word::word(*role).to_string(), json!(v));
                }
                many => {
                    // `+:1 -:5`: each part `role:pin`; a bare value is refused by the command.
                    for part in v.split_whitespace() {
                        let (role, pin) = part.split_once(':')?;
                        if !many.iter().any(|(r, _)| ecad_model::ChannelRole::exact(role) == Some(*r)) {
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
    MakePage(DeviceId),
    Checks(DeviceId),
    Place { slot: u32 },
    None,
}

/// A tile as the strip draws it, and what its links do.
fn strip_tile(t: &Tile, numbers: &[String], writable: bool, doc: &Document) -> (strip::Tile, Vec<Vec<TileLink>>) {
    let name = format!("ecad.plc.tile.{}", t.element());
    let mut lines = Vec::new();
    let mut acts = Vec::new();
    let page_links = |out: &mut Vec<Link>, acts: &mut Vec<TileLink>| {
        // Brief 056 (board 2.03): `page 5 →`.
        let mut words = pages_words(numbers);
        if let Some(last) = words.last_mut() {
            last.push_str(" \u{2192}");
        }
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
                text: "Add a card from the catalogue".into(),
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
                parts.push(Link { text: t.in_use_words(), tone: Tone::Plain, ..Link::default() });
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
    // Brief 056 (design 056 §3.2): the kind words at the head's right, the description (else the
    // type name) under the tag, the addresses under the squares; a free slot says so.
    let (kind, info, under) = match t.kind {
        TileKind::Free => (String::new(), vec![format!("{} is free", t.head)], String::new()),
        _ if t.squares.is_empty() => (t.kind_words.clone(), vec![t.title.clone(), t.range.clone()], String::new()),
        _ => (t.kind_words.clone(), vec![t.title.clone()], t.range.clone()),
    };
    let tips = match t.card() {
        Some(c) => (0..t.squares.len()).map(|i| tip_words(doc, c, t, i)).collect(),
        None => Vec::new(),
    };
    let tile = strip::Tile {
        name: name.clone(),
        head: if t.kind == TileKind::Free { String::new() } else { t.head.clone() },
        kind,
        under,
        title: if t.kind == TileKind::Free { "+".into() } else { t.tag.clone() },
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
fn tip_words(doc: &Document, c: DeviceId, t: &Tile, i: usize) -> String {
    let n = u32::try_from(i).unwrap_or(0);
    let first = ecad_engine::plc::channels::first_number(doc, c);
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
    /// Brief 056: the rack heading (design 056 §3.1): `Rack 1` and its line.
    pub rack_title: String,
    pub rack_line: String,
    /// The selected card's header (design 056 §4.1).
    pub head: Option<CardHead>,
    /// The path row of Import an I/O list or Export tags.
    pub path_row: Option<PathRow>,
    /// An import is running: its refusal shows in the path row; done, the row closes.
    importing: bool,
    /// The folder last used, while the app runs.
    pub folder: String,
    /// The project's name, for the file name offered.
    project_name: String,
    /// The `plcrack` page and its number (the footer's sentence, design 056 §8).
    pub book_page: Option<(PageId, String)>,
    /// Lay the rows out again (free channels unfolded).
    reorder: bool,
    /// Brief 056: the card editor's fields, and its folded sections (the right panel).
    pub card_editor: super::card_editor::State,
    pub card_folded: BTreeSet<String>,
}

/// The card header's words and targets (design 056 §4.1).
#[derive(Debug, Clone, PartialEq)]
pub struct CardHead {
    pub card: Option<DeviceId>,
    pub rack: Option<DeviceId>,
    /// `-A03 · output card · slot 2`.
    pub title: String,
    /// `16 channels, 9 in use, drawn in detail on page 5`.
    pub words: String,
    /// `→ Page 5`.
    pub page: Option<(PageId, String)>,
}

/// What the path row does (design 056 §6.6, §6.7).
#[derive(Debug, Clone, PartialEq)]
pub enum PathKind {
    Import,
    /// Export the rack of the selected card, or every rack.
    Export {
        rack: Option<DeviceId>,
        all: bool,
    },
}

/// The app's path row: the file and its refusal.
#[derive(Debug, Clone, PartialEq)]
pub struct PathRow {
    pub kind: PathKind,
    pub path: String,
    pub error: Option<String>,
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
            rack_title: String::new(),
            rack_line: String::new(),
            head: None,
            path_row: None,
            importing: false,
            folder: String::new(),
            project_name: String::new(),
            book_page: None,
            reorder: false,
            card_editor: Default::default(),
            card_folded: BTreeSet::new(),
        }
    }
}

impl IoView {
    /// Forget what was read: the next sync rebuilds.
    pub fn invalidate(&mut self) {
        self.built = None;
    }

    /// The selected card, if the selected tile is one.
    pub fn selected_card(&self) -> Option<DeviceId> {
        self.selected.and_then(|i| self.tiles.get(i)).and_then(Tile::card)
    }

    /// The page numbers of a tile's pages.
    fn numbers_of(doc: &Document, t: &Tile) -> Vec<String> {
        t.pages.iter().map(|p| ecad_engine::references::page_number(doc, *p)).collect()
    }

    /// Make the shown chip's tiles again.
    fn build_strip(&mut self, doc: &Document, o: &ReportModels) {
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
    fn patch_tile(&mut self, doc: &Document, o: &ReportModels, i: usize) {
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
    fn apply_scope(&mut self, cards: &[DeviceId]) {
        let only = if self.all_cards { None } else { self.selected_card() };
        if only != self.source.only() || self.source.columns().len() != columns(only.is_none()).len() || std::mem::take(&mut self.reorder) {
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
    pub fn sync(&mut self, serial: u64, doc: &Document, o: &ReportModels, writable: bool) {
        let cards: Vec<DeviceId> = iolist::cards_in_order(&o.plc).into_iter().map(|c| c.id).collect();
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
            self.after_sync(doc, o);
            return;
        }
        if o.plc.updates == self.revision {
            self.renumbered(doc);
            self.apply_scope(&cards);
            self.after_sync(doc, o);
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
        let gone: Vec<DeviceId> = self.source.by_card.keys().copied().filter(|c| !cards.contains(c)).collect();
        for c in gone {
            touched.extend(self.source.set_card(c, Vec::new()));
        }
        // The strip: the tiles of the changed cards; a changed rack or chip set makes it again.
        let changed: BTreeSet<DeviceId> = o.plc.last_changed.iter().chain(o.plc.rows_changed.iter()).copied().collect();
        if !changed.is_empty() {
            let rack_changed = changed.iter().any(|c| o.plc.racks.contains_key(c) || !o.plc.cards.contains_key(c));
            let chips_moved = overview::chips(&o.plc) != self.chips;
            let shown: Vec<usize> =
                self.tiles.iter().enumerate().filter(|(_, t)| t.card().is_some_and(|c| changed.contains(&c))).map(|(i, _)| i).collect();
            // A card that moved in or out of the shown rack changes the layout.
            let moved = changed.iter().any(|c| {
                let here = self.tiles.iter().any(|t| t.card() == Some(*c));
                let belongs = o.plc.cards.get(c).is_some_and(|card| match (self.chips.get(self.chip), card.rack) {
                    (Some(Chip::Rack(r, _)), ecad_engine::plc::rack::RackRef::Rack(x)) => *r == x,
                    (Some(Chip::NoRack(_)), ecad_engine::plc::rack::RackRef::Rack(_)) => false,
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
        self.after_sync(doc, o);
    }

    /// Brief 056: the rack heading, the card header and the book's overview page, from the kept
    /// model (words only; no rows made).
    fn after_sync(&mut self, doc: &Document, o: &ReportModels) {
        let plc = &o.plc;
        self.project_name = ecad_engine::frames::project_text(doc, ecad_engine::frame::TITLE)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "Project".into());
        // Rack heading (design 056 §3.1).
        let chip = self.chips.get(self.chip).cloned();
        (self.rack_title, self.rack_line) = match &chip {
            Some(Chip::Rack(r, _)) => {
                let n = plc.racks_by_tag().iter().position(|x| x.id == *r).map_or(1, |k| k + 1);
                let tags: Vec<&str> = self.tiles.iter().filter(|t| t.kind != TileKind::Free).map(|t| t.tag.as_str()).collect();
                let span = match (tags.first(), tags.last()) {
                    (Some(a), Some(b)) if a != b => format!("{a} to {b}"),
                    (Some(a), _) => (*a).to_string(),
                    _ => String::new(),
                };
                let used = tags.len();
                let shown = self.tiles.len();
                (format!("Rack {n}"), format!("{span} \u{b7} {used} of {shown} slots used \u{b7} {}", self.totals))
            }
            Some(Chip::NoRack(k)) => ("No rack".into(), format!("{k} {} \u{b7} {}", if *k == 1 { "card" } else { "cards" }, self.totals)),
            None => (String::new(), String::new()),
        };
        // The card header (design 056 §4.1).
        self.head = if self.all_cards {
            let cards = iolist::cards_in_order(plc);
            let (n, used) = overview::totals(&cards);
            Some(CardHead {
                card: None,
                rack: None,
                title: "All cards".into(),
                words: format!("{} {}, {}", cards.len(), if cards.len() == 1 { "card" } else { "cards" }, overview::totals_words((n, used))),
                page: None,
            })
        } else {
            self.selected_card().and_then(|c| plc.cards.get(&c)).map(|card| {
                let t = self.selected.and_then(|i| self.tiles.get(i));
                let slot = card.slot.map(|s| format!(" \u{b7} slot {s}")).unwrap_or_default();
                let used = t.map_or(0, |t| t.squares.iter().filter(|s| **s != Square::Spare).count());
                let numbers: Vec<String> = t.map(|t| Self::numbers_of(doc, t)).unwrap_or_default();
                let drawn = match numbers.as_slice() {
                    [] => "not drawn yet".to_string(),
                    [one] => format!("drawn in detail on page {one}"),
                    many => format!("drawn in detail on pages {}", many.join(", ")),
                };
                let rack = match card.rack {
                    ecad_engine::plc::rack::RackRef::Rack(r) => Some(r),
                    _ => None,
                };
                CardHead {
                    card: Some(card.id),
                    rack,
                    title: format!("{} \u{b7} {}{slot}", card.tag, overview::type_name(card.ty).to_lowercase()),
                    words: format!("{} channels, {used} in use, {drawn}", card.channels),
                    page: t.and_then(|t| t.pages.first().copied()).zip(numbers.first().cloned()),
                }
            })
        };
        // The book's overview page (design 056 §8).
        self.book_page = doc
            .page_order
            .iter()
            .find(|p| ecad_engine::generated::kind_of(doc, **p) == Some(ecad_model::GenKind::PlcRack))
            .map(|p| (*p, ecad_engine::references::page_number(doc, *p)));
    }

    /// The file name offered for a tag table (design 056 §6.6): `<project> PLC tags.csv` in the
    /// folder last used.
    fn offered_path(&self) -> String {
        let name = format!("{} PLC tags.csv", self.project_name);
        if self.folder.is_empty() { name } else { std::path::Path::new(&self.folder).join(name).to_string_lossy().into_owned() }
    }

    /// Remember the folder of `path`.
    fn remember(&mut self, path: &str) {
        if let Some(d) = std::path::Path::new(path).parent().filter(|d| !d.as_os_str().is_empty()) {
            self.folder = d.to_string_lossy().into_owned();
        }
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
        // Brief 056: an import's refusal stays in its path row; done, the row closes.
        if std::mem::take(&mut self.importing) {
            match refusal {
                Some(m) => {
                    if let Some(r) = self.path_row.as_mut() {
                        r.error = Some(m);
                    }
                }
                None => self.path_row = None,
            }
            return;
        }
        if let (Some((k, col)), Some(msg)) = (self.pending.take(), refusal) {
            self.table.refuse(k, col, msg);
        }
    }

    /// Draw the rack heading, the strip, the card header and the table (or Compare). Returns what
    /// it asks for.
    pub fn show(&mut self, ui: &mut eframe::egui::Ui) -> Vec<ListAction> {
        use eframe::egui;
        let mut out = Vec::new();
        let s = ecad_ui::SHELL;
        let ty = ecad_ui::tokens::ui_type::BODY;
        let small = ecad_ui::tokens::ui_type::SMALL;
        // Brief 056 (design 056 §3.1): `Rack 1  -A01 to -A06 · 6 of 7 slots used · …`, the legend
        // and Add a rack at the right; the chips when there is more than one rack.
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
                ui.add_space(ecad_ui::tokens::space::M);
            }
            let title = ecad_ui::fonts::font(ui.ctx(), ecad_ui::tokens::ui_type::TITLE);
            ui.label(egui::RichText::new(&self.rack_title).font(title).color(ecad_ui::color(s.text)));
            ui.label(egui::RichText::new(&self.rack_line).size(ty.size).color(ecad_ui::color(s.text_2)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(ecad_ui::tokens::space::M);
                let add = ui.push_id(ADD_RACK, |ui| {
                    ui.add_enabled_ui(self.source.writable, |ui| ecad_ui::prim::button(ui, "Add a rack", ecad_ui::prim::Kind::Secondary)).inner
                });
                if add.inner.on_disabled_hover_text(WRITES_OFF).clicked() {
                    out.push(ListAction::AddRack);
                }
                if !self.tiles.is_empty() {
                    ui.add_space(ecad_ui::tokens::space::M);
                    // The legend's own width, laid left to right at the right edge.
                    let w = 300.0_f32.min(ui.available_width());
                    ui.allocate_ui_with_layout(egui::vec2(w, ui.available_height()), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        strip::legend(ui, &LEGEND)
                    });
                }
            });
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
        ui.add_space(ecad_ui::tokens::space::M);
        let r = ui.available_rect_before_wrap();
        ui.painter().hline(r.x_range(), r.top(), eframe::egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
        ui.add_space(ecad_ui::tokens::space::S);
        // The card header (design 056 §4.1).
        ui.push_id(CARD_HEAD, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(ecad_ui::tokens::space::M);
                if let Some(h) = &self.head {
                    let strong = ecad_ui::fonts::font(ui.ctx(), ecad_ui::tokens::ui_type::BODY_EMPHASIS);
                    ui.label(egui::RichText::new(&h.title).font(strong).color(ecad_ui::color(s.text)));
                    ui.label(egui::RichText::new(&h.words).size(ty.size).color(ecad_ui::color(s.text_2)));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(ecad_ui::tokens::space::M);
                    let Some(h) = self.head.clone() else { return };
                    if let Some((pg, n)) = &h.page {
                        let go =
                            ui.push_id(GO_PAGE, |ui| ecad_ui::prim::button(ui, &format!("\u{2192} Page {n}"), ecad_ui::prim::Kind::Secondary)).inner;
                        if go.clicked() {
                            out.push(ListAction::GoToCard(*pg, h.card));
                        }
                    }
                    let ex = ui.push_id(EXPORT_TAGS, |ui| ecad_ui::prim::button(ui, "Export tags", ecad_ui::prim::Kind::Secondary)).inner;
                    if ex.clicked() {
                        out.push(ListAction::ExportTags);
                    }
                    ui.add_space(ecad_ui::tokens::space::S);
                    if let Some(c) = h.card {
                        ui.push_id(MAKE_TAGS, |ui| {
                            ui.menu_button("\u{2026}", |ui| {
                                let b = ui.add_enabled(self.source.writable, egui::Button::new("Make tags"));
                                if b.on_disabled_hover_text(WRITES_OFF).clicked() {
                                    out.push(ListAction::Run(format!("plcmaketags {}", json!({"card": c.hex()}))));
                                    ui.close();
                                }
                            });
                        });
                    }
                    let mut i = usize::from(self.all_cards);
                    ui.push_id(SCOPE, |ui| ecad_ui::prim::segmented(ui, &mut i, &[THIS_CARD, ALL_CARDS]));
                    if (i == 1) != self.all_cards {
                        out.push(ListAction::Scope(i == 1));
                    }
                });
            });
        });
        // The path row of Import tags or Export tags (design 056 §6.6, §6.7).
        if let Some(row) = self.path_row.as_mut() {
            let mut close = false;
            ui.horizontal(|ui| {
                ui.add_space(ecad_ui::tokens::space::M);
                let label = match row.kind {
                    PathKind::Import => "Fill tags and descriptions from",
                    PathKind::Export { .. } => "Save the tag table as",
                };
                ui.label(egui::RichText::new(label).size(ty.size).color(ecad_ui::color(s.text_2)));
                // Three label columns wide: room for a path, in the components' sizes.
                ecad_ui::control::TextEntry::new(&mut row.path, "Tag table file")
                    .placeholder("Tag table file")
                    .width(3.0 * ecad_ui::tokens::size::LABEL_COLUMN)
                    .show(ui);
                let ok = match &mut row.kind {
                    PathKind::Import => ecad_ui::prim::button(ui, "Import", ecad_ui::prim::Kind::Primary).clicked(),
                    PathKind::Export { all, .. } => {
                        ecad_ui::control::Toggle::new(all, "All racks").show(ui);
                        ui.label(egui::RichText::new("All racks").size(ty.size).color(ecad_ui::color(s.text_2)));
                        ecad_ui::prim::button(ui, "Export", ecad_ui::prim::Kind::Primary).clicked()
                    }
                };
                if ecad_ui::prim::button(ui, "Cancel", ecad_ui::prim::Kind::Quiet).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
                if ok && !row.path.trim().is_empty() {
                    out.push(ListAction::PathOk);
                }
                if let Some(e) = &row.error {
                    ui.label(egui::RichText::new(e).size(small.size).color(ecad_ui::color(s.error)));
                }
            });
            if close {
                self.path_row = None;
            }
        }
        let folded = self.source.folded;
        // With channels folded, the table is as tall as its rows, so `6 more free channels.`
        // sits under the last one (board 2.03); else it takes the room left.
        let events = if folded > 0 {
            let rows = self.source.keys().len() as f32 + 1.0;
            let h = (rows * TABLE_ROW_PX + ecad_ui::tokens::space::M).min((ui.available_height() - 2.0 * TABLE_ROW_PX).max(TABLE_ROW_PX));
            ui.allocate_ui(egui::vec2(ui.available_width(), h), |ui| {
                ui.set_max_height(h);
                self.table.show(ui, &self.source)
            })
            .inner
        } else {
            self.table.show(ui, &self.source)
        };
        for e in events {
            match e {
                TableEvent::Commit { key, col, value } if self.source.writable => {
                    if let Some(line) = self.source.row(key).and_then(|r| edit_line(r, col, &value)) {
                        self.pending = Some((key, col));
                        out.push(ListAction::Run(line));
                    }
                }
                TableEvent::GoTo { key, col } if col == DRIVES => {
                    if let Some((pg, at)) = self.source.row(key).and_then(|r| signals::drives(&r.targets).0.first().and_then(|t| t.page.zip(t.at))) {
                        out.push(ListAction::GoTo(pg, at));
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
        // `6 more free channels.` (design 056 §4.2): a click unfolds them.
        if folded > 0 {
            ui.horizontal(|ui| {
                ui.add_space(ecad_ui::tokens::space::M);
                let words = if folded == 1 { "1 more free channel.".to_string() } else { format!("{folded} more free channels.") };
                let r = ui.push_id(MORE_FREE, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(words).size(ty.size).color(ecad_ui::color(s.text_3))).sense(egui::Sense::click()))
                });
                if r.inner.clicked() {
                    out.push(ListAction::Unfold);
                }
            });
        }
        out
    }

    /// Act on what [`IoView::show`] asked that the view itself answers (tile, chip, scope, reveal,
    /// Compare, the path row); the rest goes back to the app.
    pub fn handle(&mut self, a: ListAction) -> Option<ListAction> {
        match a {
            ListAction::Tile(i) => {
                let changed = self.selected != Some(i);
                self.select(i);
                if changed {
                    self.source.unfold = false;
                }
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
            ListAction::Unfold => {
                self.source.unfold = true;
                self.reorder = true;
                None
            }
            ListAction::ImportTags => {
                let path = if self.folder.is_empty() { String::new() } else { format!("{}{}", self.folder, std::path::MAIN_SEPARATOR) };
                self.path_row = Some(PathRow { kind: PathKind::Import, path, error: None });
                None
            }
            ListAction::ExportTags => {
                let rack = self.head.as_ref().and_then(|h| h.rack);
                self.path_row = Some(PathRow { kind: PathKind::Export { rack, all: rack.is_none() }, path: self.offered_path(), error: None });
                None
            }
            ListAction::PathOk => {
                let row = self.path_row.clone()?;
                let path = row.path.trim().to_string();
                self.remember(&path);
                match row.kind {
                    // Brief 056 (owner, 2026-11-14): one way, one transaction; `u` undoes it.
                    PathKind::Import => {
                        self.importing = true;
                        if let Some(r) = self.path_row.as_mut() {
                            r.error = None;
                        }
                        Some(ListAction::Run(format!("plcimport {}", json!({"path": path}))))
                    }
                    PathKind::Export { rack, all } => {
                        self.path_row = None;
                        let mut p = json!({"path": path});
                        if let (Some(r), false) = (rack, all) {
                            p["rack"] = json!(r.hex());
                        }
                        Some(ListAction::Run(format!("plcexport {p}")))
                    }
                }
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
        out.extend(doc.representations_of(ecad_model::Subject::Device(c)).filter(|(_, r)| r.page == *page).map(|(id, _)| id));
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
