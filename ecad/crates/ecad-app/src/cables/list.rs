//! Project ▸ Cables (UI rev 3, board 2.05): every cable with type, cores, length, ends, cores used
//! and checks, as the one table over the cable report (`ReportModels::cable_table`); under it the
//! selected cable's core table (`ReportModels::core_table`). Brief 057 (board 2.06): the cable drawn end
//! to end ([`crate::cables::view`]) with each core in its cable type's colour; the glands' sizes come
//! with the cable manager (2.7) and show a dash. Brief 059 (the cable manager): a cable not drawn
//! yet shows its planned ends and a `not drawn` badge; **+ Cable** builds one; the Details pane
//! edits it ([`crate::cables::editor`]).
//!
//! **Update map:** see [`crate::reports::report`]: the cable copy is taken again only when the report
//! models' revision changed, and only while the tab is shown; the core table also when another
//! cable is selected.

use ecad_electrical::reports::kept::ReportModels;
use ecad_model::CableId;
use ecad_widgets::prim::Tone;
use ecad_widgets::table::{Cell, Changed, Column, ColumnId, Table, TableEvent, TableSource, Width};

use crate::reports::report::{From, ReportColumns, ReportRows};

pub const CABLE: ColumnId = ColumnId(0);
pub(crate) const TYPE: ColumnId = ColumnId(1);
pub const CORES: ColumnId = ColumnId(2);
pub(crate) const LENGTH: ColumnId = ColumnId(3);
pub const FROM: ColumnId = ColumnId(4);
pub const TO: ColumnId = ColumnId(5);
pub(crate) const GLANDS: ColumnId = ColumnId(6);
pub(crate) const USED: ColumnId = ColumnId(7);
pub(crate) const CHECKS: ColumnId = ColumnId(8);

/// The details pane's split handle (accessible name) and where its size is remembered.
pub const SPLIT: &str = "ecad.lists.cables.split";
const SPLIT_KEY: &str = "ecad.lists.cables.details_split";

/// The two views of the selected cable (board 2.06).
pub(crate) const VIEWS: [&str; 2] = ["Cable view", "Cores only"];

pub fn columns() -> Vec<Column> {
    vec![
        Column::read(CABLE.0, "Cable", Width::Initial(90.0)),
        Column::read(TYPE.0, "Type", Width::Initial(150.0)),
        Column::read(CORES.0, "Cores", Width::Initial(110.0)),
        Column::read(LENGTH.0, "Length", Width::Initial(80.0)),
        Column::read(FROM.0, "From", Width::Initial(390.0)),
        Column::read(TO.0, "To", Width::Initial(390.0)),
        Column::read(GLANDS.0, "Glands", Width::Initial(120.0)).unsorted(),
        Column::read(USED.0, "Cores used", Width::Initial(110.0)),
        Column::read(CHECKS.0, "Checks", Width::Remainder),
    ]
}

/// The cable report as a table source.
#[derive(Debug)]
pub struct CablesSource {
    cols: Vec<Column>,
    pub rows: ReportRows,
    /// Brief 057: each row's cable, and the rating results.
    pub ids: Vec<CableId>,
    pub rating: crate::wires::list::RatingCopy,
    /// Brief 057 (owner rule): each row's open cores, read with the copy.
    pub open: Vec<Vec<String>>,
    /// Brief 059: each row's drawn state and planned ends, from the cable rows.
    pub info: Vec<RowInfo>,
}

/// Brief 059: what a row shows beyond the report's columns.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RowInfo {
    pub drawn: bool,
    /// Each end's plan (`=PUMP +CAB1`), from, to.
    pub planned: [Option<String>; 2],
    pub glands: [Option<String>; 2],
}

/// Brief 059 (design 059 §7): the empty list's words.
pub const EMPTY: &str = "No cables yet. + Cable builds one; a slash across wires draws one.";
/// Brief 059 (design 059 §5.1): the toolbar's button and its menu.
pub const ADD: &str = "+ Cable";
pub const NEW_CABLE: &str = "New cable";
pub const ASSIGN_HEADER: &str = "Assign in the drawing";

impl Default for CablesSource {
    fn default() -> Self {
        Self { cols: columns(), rows: ReportRows::default(), ids: Vec::new(), rating: Default::default(), open: Vec::new(), info: Vec::new() }
    }
}

impl CablesSource {
    /// Brief 057: row `k`'s rating result.
    pub fn rating(&self, k: usize) -> Option<&ecad_electrical::circuits::circuit::Rating> {
        self.ids.get(k).and_then(|c| self.rating.results.get(&ecad_electrical::circuits::RatedSubject::Cable(*c)))
    }

    /// Brief 059: whether row `k`'s cable is drawn (unknown rows count as drawn).
    pub fn drawn(&self, k: usize) -> bool {
        self.info.get(k).is_none_or(|i| i.drawn)
    }
}

impl ReportColumns for CablesSource {
    fn rows(&self) -> &ReportRows {
        &self.rows
    }
    fn cols(&self) -> &[Column] {
        &self.cols
    }
    fn from(&self, col: ColumnId) -> From {
        match col {
            CABLE => From::Header("Tag"),
            TYPE => From::Header("Type"),
            CORES => From::Header("Cores"),
            // Brief 059: a cable not drawn shows its planned ends.
            FROM | TO => From::Derived,
            // Brief 059 (059d): each end's gland, `M20, M20`.
            GLANDS => From::Derived,
            _ => From::Derived,
        }
    }
    fn derived(&self, k: usize, col: ColumnId) -> Cell {
        let r = &self.rows;
        match col {
            FROM | TO if self.drawn(k) => r.cell(k, if col == FROM { "From" } else { "To" }),
            FROM | TO => {
                let side = usize::from(col == TO);
                match self.info.get(k).and_then(|i| i.planned.get(side).cloned().flatten()) {
                    Some(p) => Cell::read(format!("{p} (planned)")),
                    None => Cell::read(crate::reports::report::DASH),
                }
            }
            CHECKS if !self.drawn(k) => Cell::Badge("not drawn".into(), Tone::Neutral),
            GLANDS => match self.info.get(k).map(|i| &i.glands) {
                Some([None, None]) | None => Cell::read(crate::reports::report::DASH),
                Some([a, b]) => {
                    let d = crate::reports::report::DASH;
                    Cell::Text(format!("{}, {}", a.as_deref().unwrap_or(d), b.as_deref().unwrap_or(d)))
                }
            },
            LENGTH => match r.get(k, "Length").filter(|c| !c.text.is_empty()) {
                Some(c) => Cell::Text(c.text.clone()),
                None => Cell::read(crate::reports::report::DASH),
            },
            USED => Cell::Text(format!("{} of {}", r.text(k, "Used"), r.text(k, "Cores"))),
            CHECKS => {
                let spare = r.text(k, "Spare");
                // Brief 057 (owner rule): open cores are errors, first.
                if let Some(n) = self.open.get(k).map(Vec::len).filter(|n| *n > 0) {
                    return Cell::Badge(format!("{n} error{}", if n == 1 { "" } else { "s" }), Tone::Error);
                }
                // Brief 057 (design 057 §2.2): a rating row comes first.
                if let Some(rt) = self.rating(k)
                    && let Some(w) = rt.status()
                {
                    return Cell::Badge(w, if rt.fails() { Tone::Error } else { Tone::Warning });
                }
                match r.flag(k) {
                    Some(_) => Cell::Badge("check".into(), Tone::Warning),
                    None if spare.parse::<u32>().is_ok_and(|n| n > 0) => {
                        Cell::Read { text: format!("{spare} spare core{}", if spare == "1" { "" } else { "s" }), go: false }
                    }
                    None => Cell::Badge("ok".into(), Tone::Ok),
                }
            }
            _ => Cell::Empty,
        }
    }
}

impl TableSource for CablesSource {
    type Key = usize;
    fn columns(&self) -> &[Column] {
        &self.cols
    }
    fn keys(&self) -> &[usize] {
        &self.rows.keys
    }
    fn contains(&self, key: usize) -> bool {
        key < self.rows.rows.len()
    }
    fn cell(&self, key: usize, col: ColumnId) -> Cell {
        crate::reports::report::cell(self, key, col)
    }
}

pub const CORE: ColumnId = ColumnId(0);
pub const COLOUR: ColumnId = ColumnId(1);
pub(crate) const CABINET: ColumnId = ColumnId(2);
pub const WIRE: ColumnId = ColumnId(3);
pub const FIELD: ColumnId = ColumnId(4);
pub const STATUS: ColumnId = ColumnId(5);
/// Brief 057 (board 2.06): the core size, the wire beyond the field end, and the Not used tick.
pub const SIZE: ColumnId = ColumnId(6);
pub(crate) const FIELD_WIRE: ColumnId = ColumnId(7);
pub(crate) const UNUSED: ColumnId = ColumnId(8);
/// The Not used tick: a plain toggle in the core table (the column's header names it), the
/// shared `Cell::Toggle`; its accessible name.
pub const NOT_USED_TOGGLE: &str = "Toggle";

/// One cable's cores (`ReportModels::core_table`).
#[derive(Debug)]
pub struct CoresSource {
    cols: Vec<Column>,
    pub rows: ReportRows,
    /// Brief 057: the selected cable's cable type colours, in core order; its core size; per
    /// row, the wire beyond the field end; the cores ticked Not used.
    pub colours: Vec<String>,
    pub size: String,
    pub field_wires: Vec<String>,
    pub unused: std::collections::BTreeSet<String>,
    /// Brief 059: whether the selected cable is drawn.
    pub drawn: bool,
}

impl CoresSource {
    /// Row `k`'s core colour (its label when that is a colour code, else the cable type's).
    pub fn colour(&self, k: usize) -> Option<String> {
        crate::cables::view::core_colour(&self.rows.text(k, "Core"), k, &self.colours)
    }

    /// Whether row `k`'s core is ticked Not used.
    pub fn is_unused(&self, k: usize) -> bool {
        self.unused.contains(&self.rows.text(k, "Core"))
    }

    /// Whether row `k`'s core is open (owner rule): not connected at both ends, not ticked.
    pub fn is_open(&self, k: usize) -> bool {
        let r = &self.rows;
        let spare = r.text(k, "State") == "spare";
        !self.is_unused(k) && (spare || r.text(k, "From").is_empty() || r.text(k, "To").is_empty())
    }
}

impl Default for CoresSource {
    fn default() -> Self {
        Self {
            cols: vec![
                Column::read(CORE.0, "Core", Width::Initial(80.0)),
                Column::read(COLOUR.0, "Colour", Width::Initial(110.0)).unsorted(),
                Column::read(SIZE.0, "Size", Width::Initial(90.0)).unsorted(),
                Column::read(CABINET.0, "Cabinet end", Width::Initial(300.0)),
                Column::read(WIRE.0, "Wire", Width::Initial(120.0)),
                Column::read(FIELD.0, "Field end", Width::Initial(300.0)),
                Column::read(FIELD_WIRE.0, "Wire", Width::Initial(120.0)),
                Column::read(STATUS.0, "Status", Width::Initial(150.0)),
                Column::read(UNUSED.0, "Not used", Width::Remainder).unsorted(),
            ],
            rows: ReportRows::default(),
            colours: Vec::new(),
            size: String::new(),
            field_wires: Vec::new(),
            unused: Default::default(),
            drawn: true,
        }
    }
}

impl ReportColumns for CoresSource {
    fn rows(&self) -> &ReportRows {
        &self.rows
    }
    fn cols(&self) -> &[Column] {
        &self.cols
    }
    fn from(&self, col: ColumnId) -> From {
        match col {
            CORE => From::Header("Core"),
            WIRE => From::Header("Wire number"),
            FIELD => From::Header("To"),
            _ => From::Derived,
        }
    }
    fn derived(&self, k: usize, col: ColumnId) -> Cell {
        let dash = || Cell::read(crate::reports::report::DASH);
        match (col, self.rows.text(k, "State").as_str()) {
            // Board 2.06: the colour's bar, then its code.
            (COLOUR, _) => self.colour(k).map_or_else(dash, |c| Cell::Colour(crate::cables::view::bars(&c), c)),
            (SIZE, _) if self.size.is_empty() => dash(),
            (SIZE, _) => Cell::Text(format!("{} mm\u{b2}", self.size)),
            // An unwired offered core has no cabinet end (owner rule: it is open, not "spare").
            (CABINET, "spare") => Cell::Empty,
            (CABINET, _) => Cell::Text(self.rows.text(k, "From")),
            (FIELD_WIRE, _) => self.field_wires.get(k).filter(|w| !w.is_empty()).map_or(Cell::Empty, |w| Cell::Text(w.clone())),
            (STATUS, _) if self.is_unused(k) => Cell::read("not used"),
            // Brief 059 (design 059 §5.5): an unwired core is "not assigned": read on a cable not
            // drawn, an error on a drawn one (057's owner rule).
            (STATUS, "spare") if !self.drawn => Cell::read("not assigned"),
            (STATUS, "spare") => Cell::Badge("not assigned".into(), Tone::Error),
            (STATUS, _) if self.is_open(k) => Cell::Badge("open end".into(), Tone::Error),
            (STATUS, "used") => Cell::Badge("connected".into(), Tone::Ok),
            (STATUS, other) => Cell::Badge(other.to_string(), Tone::Warning),
            (UNUSED, _) => Cell::Toggle(self.is_unused(k)),
            _ => Cell::Empty,
        }
    }
}

impl TableSource for CoresSource {
    type Key = usize;
    fn columns(&self) -> &[Column] {
        &self.cols
    }
    fn keys(&self) -> &[usize] {
        &self.rows.keys
    }
    fn contains(&self, key: usize) -> bool {
        key < self.rows.rows.len()
    }
    fn cell(&self, key: usize, col: ColumnId) -> Cell {
        crate::reports::report::cell(self, key, col)
    }
}

/// The Cables tab's state.
#[derive(Debug)]
pub struct CablesView {
    pub source: CablesSource,
    pub table: Table<usize>,
    /// Each row's cable and where it is drawn.
    pub ids: Vec<CableId>,
    pub gos: Vec<Option<(ecad_model::PageId, ecad_model::Pt)>>,
    pub cores: CoresSource,
    pub core_table: Table<usize>,
    /// The cable whose cores are shown, and the copy it was read at.
    cores_of: Option<(CableId, (u64, u64))>,
    seen: Option<(u64, u64)>,
    pub copies: u64,
    /// Brief 057 (board 2.06): Cable view (0) or Cores only (1), the ends swapped, and the
    /// selected cable's colours as read at a change count.
    pub view: usize,
    pub swapped: bool,
    /// Brief 057 (owner): the details pane below the list (closed: a plain list).
    pub details: bool,
    colours_of: Option<(CableId, u64, Option<(CableId, (u64, u64))>)>,
    /// Brief 059: the selected cable's editor, a cable to select once the list has it (after
    /// + Cable), and the cables not drawn (+ Cable's menu: cable, its line).
    pub edit: crate::cables::editor::Editor,
    pub pending_select: Option<CableId>,
    pub undrawn: Vec<(CableId, String)>,
    /// Brief 059 (owner, board 2.06): the Details pane's tab, Cable (0) or Checks (1), and the
    /// selected cable's Checks rows with their place (`2/E3`), read with its copy.
    pub detail_tab: usize,
    pub cable_checks: Vec<(ecad_electrical::checks::Check, String)>,
    checks_of: Option<(CableId, u64)>,
}

impl Default for CablesView {
    fn default() -> Self {
        let mut table = Table::new("ecad.lists.cables", "cables");
        table.toolbar = false;
        table.empty = EMPTY;
        let mut core_table = Table::new("ecad.lists.cables.cores", "cores");
        core_table.toolbar = false;
        core_table.empty = "Select a cable to see its cores.";
        Self {
            source: CablesSource::default(),
            table,
            ids: Vec::new(),
            gos: Vec::new(),
            cores: CoresSource::default(),
            core_table,
            cores_of: None,
            seen: None,
            copies: 0,
            view: 0,
            swapped: false,
            details: false,
            colours_of: None,
            edit: Default::default(),
            pending_select: None,
            undrawn: Vec::new(),
            detail_tab: 0,
            cable_checks: Vec::new(),
            checks_of: None,
        }
    }
}

impl CablesView {
    /// Bring the copies up to date (the cables when the models' revision changed; the cores also
    /// when the selected cable changed).
    pub fn sync(&mut self, serial: u64, o: &ReportModels) {
        let key = (serial, o.build_id ^ o.revision);
        if self.seen != Some(key) {
            self.seen = Some(key);
            self.source.rows.take(o.cable_table());
            let rows = o.cables();
            self.ids = rows.iter().map(|r| r.cable).collect();
            self.source.ids = self.ids.clone();
            self.gos = rows.iter().map(|r| r.go).collect();
            self.source.info = rows.iter().map(|r| RowInfo { drawn: r.drawn, planned: r.planned.clone(), glands: r.glands.clone() }).collect();
            // Brief 059: + Cable's menu lists the cables not drawn (`-W03  H07RN-F 4G1.5 · +CAB1 → +FIELD`).
            self.undrawn = rows.iter().filter(|r| !r.drawn).map(|r| (r.cable, undrawn_line(r))).collect();
            self.undrawn.sort_by(|a, b| a.1.cmp(&b.1));
            self.copies += 1;
            self.table.update(&self.source, Changed::All);
            if let Some(want) = self.pending_select
                && let Some(k) = self.ids.iter().position(|c| *c == want)
            {
                self.pending_select = None;
                self.table.selection.set([k]);
                self.table.reveal(&[k]);
            }
            // The first cable is shown until one is picked (board 2.05).
            if self.table.selection.is_empty()
                && let Some(first) = self.table.view.visible_keys().first().copied()
            {
                self.table.selection.set([first]);
            }
        }
        let selected = self.selected().and_then(|k| self.ids.get(k).copied());
        let want = selected.map(|c| (c, key));
        if want != self.cores_of {
            self.cores_of = want;
            match selected {
                Some(c) => self.cores.rows.take(o.core_table(c)),
                None => self.cores.rows = ReportRows::default(),
            }
            self.core_table.update(&self.cores, Changed::All);
        }
    }

    /// Brief 057: bring the rating copy up to date and patch the Checks cells of the re-checked
    /// cables only.
    pub(crate) fn sync_rating(&mut self, serial: u64, kept: &ecad_electrical::circuits::kept::Kept) {
        match self.source.rating.sync(serial, kept) {
            None => self.table.update(&self.source, Changed::All),
            Some(v) if v.is_empty() => {}
            Some(v) => {
                let rows: Vec<usize> = self
                    .ids
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| v.contains(&ecad_electrical::circuits::RatedSubject::Cable(**c)))
                    .map(|(k, _)| k)
                    .collect();
                if !rows.is_empty() {
                    self.table.update(&self.source, Changed::Keys(rows));
                }
            }
        }
    }

    /// Brief 057: the selected cable's core colours, size, ticks and far-side wires, and each
    /// cable's open cores; read again only when the selection or the project changed (as the
    /// cable copy itself is).
    pub(crate) fn sync_colours(&mut self, doc: &ecad_model::Document, count: u64) {
        use ecad_electrical::styles::cable;
        let selected = self.selected().and_then(|k| self.ids.get(k).copied());
        let want = selected.map(|c| (c, count, self.cores_of));
        if want == self.colours_of {
            return;
        }
        self.colours_of = want;
        let c = &mut self.cores;
        c.colours = selected.map(|k| cable::core_colours(doc, k)).unwrap_or_default();
        // Each cable's open cores, read when the project changed (as the cable copy is).
        // Brief 059: a cable not drawn has no open cores.
        let open: Vec<Vec<String>> = self
            .ids
            .iter()
            .map(|k| {
                if ecad_electrical::cables::is_drawn(doc, *k) {
                    cable::open_cores(doc, *k).into_iter().map(|o| o.label).collect()
                } else {
                    Vec::new()
                }
            })
            .collect();
        if open != self.source.open {
            self.source.open = open;
            self.table.update(&self.source, Changed::All);
        }
        // Brief 059: the editor's copy of the selected cable.
        match selected {
            Some(k) => self.edit.set_copy(crate::cables::editor::copy(doc, k)),
            None => self.edit.set_copy(crate::cables::editor::Copy::default()),
        }
        let c = &mut self.cores;
        c.drawn = selected.is_none_or(|k| ecad_electrical::cables::is_drawn(doc, k));
        c.size = selected.and_then(|k| cable_size(doc, k)).unwrap_or_default();
        c.unused = selected
            .and_then(|k| doc.cables.get(&k))
            .map(|x| {
                // Design 072 §4.5: Not used is the stored core's purpose.
                x.cores.iter().filter(|m| m.is_spare()).filter_map(|m| m.label_str().map(str::to_string)).collect()
            })
            .unwrap_or_default();
        let rows = c.rows.rows.len();
        c.field_wires = (0..rows).map(|k| selected.map(|cab| beyond(doc, cab, &c.rows.text(k, "To"))).unwrap_or_default()).collect();
        self.core_table.update(&self.cores, Changed::All);
    }

    /// Brief 059 (board 2.06): the selected cable's Checks rows (the kept list's rows about it),
    /// read again only when the selection or the project's change count changed.
    pub(crate) fn sync_checks(&mut self, p: &mut ecad_engine::project::Project, count: u64) {
        let selected = self.selected().and_then(|k| self.ids.get(k).copied());
        let want = selected.map(|k| (k, count));
        if want == self.checks_of {
            return;
        }
        self.checks_of = want;
        self.cable_checks.clear();
        let Some(k) = selected else { return };
        let rows: Vec<ecad_electrical::checks::Check> =
            p.checks().rows().filter(|c| c.object == ecad_electrical::checks::Object::Cable(k)).cloned().collect();
        let doc = p.ledger.document();
        let style = doc.settings.reference_style();
        let mut rows = ecad_electrical::checks::sort(doc, rows);
        rows.truncate(200);
        self.cable_checks = rows
            .into_iter()
            .map(|c| {
                let place = match (c.page, c.at) {
                    (Some(pg), Some(at)) => ecad_model::connectivity::wiring::place(doc, pg, at).map(|x| x.text(style)).unwrap_or_default(),
                    _ => String::new(),
                };
                (c, place)
            })
            .collect();
    }

    /// The selected cable as drawn (board 2.06).
    pub fn drawing(&self, k: usize) -> crate::cables::view::Drawing {
        let rows = &self.source.rows;
        let words =
            [rows.text(k, "Type"), rows.text(k, "Length")].into_iter().filter(|w| !w.is_empty() && w != "?").collect::<Vec<_>>().join(" \u{b7} ");
        let c = &self.cores.rows;
        let cores = c
            .keys
            .iter()
            .map(|i| crate::cables::view::Core {
                label: c.text(*i, "Core"),
                colour: self.cores.colour(*i),
                wire: c.text(*i, "Wire number"),
                from: if c.text(*i, "State") == "spare" { String::new() } else { c.text(*i, "From") },
                to: c.text(*i, "To"),
                field_wire: self.cores.field_wires.get(*i).cloned().unwrap_or_default(),
                unused: self.cores.is_unused(*i),
            })
            .collect();
        // Brief 059: a cable not drawn shows its plan at each end.
        let info = self.source.info.get(k).cloned().unwrap_or(RowInfo { drawn: true, ..RowInfo::default() });
        let (from, to) = if info.drawn {
            (rows.text(k, "From"), rows.text(k, "To"))
        } else {
            (info.planned[0].clone().unwrap_or_default(), info.planned[1].clone().unwrap_or_default())
        };
        let glands = [info.glands[0].clone().unwrap_or_default(), info.glands[1].clone().unwrap_or_default()];
        crate::cables::view::Drawing { tag: rows.text(k, "Tag"), words, from, to, cores, not_drawn: !info.drawn, glands }
    }

    /// Forget the copies.
    pub fn invalidate(&mut self) {
        self.seen = None;
        self.cores_of = None;
        self.colours_of = None;
        self.table.reset();
        self.core_table.reset();
    }

    /// The selected cable's row.
    pub fn selected(&self) -> Option<usize> {
        self.table.selection.cursor().or_else(|| self.table.selection.keys().iter().next().copied())
    }

    /// The footer's counts: cables, cables with a check, cables without a length.
    pub fn counts(&self) -> (usize, usize, usize) {
        let keys = &self.source.rows.keys;
        let flagged = keys.iter().filter(|k| self.source.rows.flag(**k).is_some()).count();
        let lengths = self.source.rows.headers.contains(&"Length");
        let no_length = keys.iter().filter(|k| !lengths || self.source.rows.text(**k, "Length").is_empty()).count();
        (keys.len(), flagged, no_length)
    }

    /// The cable table. Returns where to go. A double-click opens the details (brief 057, owner).
    pub(crate) fn show_cables(&mut self, ui: &mut eframe::egui::Ui) -> Option<(ecad_model::PageId, ecad_model::Pt)> {
        let mut go = None;
        for e in self.table.show(ui, &self.source) {
            match e {
                TableEvent::Open(_) => self.details = true,
                TableEvent::GoTo { key, .. } => go = self.gos.get(key).copied().flatten(),
                TableEvent::Export(csv) => crate::workspace::copy_csv(ui, csv),
                _ => {}
            }
        }
        go
    }

    /// The selected cable's core table. Returns a Not used tick asked for: the core's label and
    /// whether it is now not used.
    pub fn show_cores(&mut self, ui: &mut eframe::egui::Ui) -> Option<(String, bool)> {
        let mut tick = None;
        for e in self.core_table.show(ui, &self.cores) {
            match e {
                TableEvent::Export(csv) => crate::workspace::copy_csv(ui, csv),
                TableEvent::Button { key, col: UNUSED, .. } => tick = Some((self.cores.rows.text(key, "Core"), !self.cores.is_unused(key))),
                _ => {}
            }
        }
        tick
    }
}

/// The Cables tab (board 2.06): the cable table, the selected cable's line and its drawing, then
/// its cores. Returns where to go.
pub(crate) fn show_tab(t: &mut crate::shell::app::App, ui: &mut eframe::egui::Ui) -> Option<(ecad_model::PageId, ecad_model::Pt)> {
    use ecad_widgets::tokens::{size, space, ui_type};
    use eframe::egui::{self, vec2};
    let s = ecad_widgets::SHELL;
    let r = t.reports.as_mut()?;
    let v = &mut r.cables;
    let full = ui.available_rect_before_wrap();
    // Brief 057 (owner): a plain list until Details (or a double-click) opens the selected cable
    // below it; Esc or Details again closes it. The split is remembered per user.
    if v.details && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        v.details = false;
    }
    if !v.details || v.selected().is_none() {
        let go = ui.allocate_ui(full.size(), |ui| v.show_cables(ui)).inner;
        return go;
    }
    let split_id = egui::Id::unique(SPLIT_KEY);
    let frac: f32 = ui.data_mut(|d| d.get_persisted(split_id)).unwrap_or(0.35_f32).clamp(0.15, 0.8);
    let table_h = (full.height() * frac).max(3.0 * size::ROW_TABLE);
    let mut go = ui
        .allocate_ui(vec2(full.width(), table_h), |ui| {
            ui.set_min_height(table_h);
            v.show_cables(ui)
        })
        .inner;
    // The split handle (the shared splitter): drag to resize the list against the details.
    let resp = ecad_widgets::splitter::horizontal(ui, SPLIT);
    if resp.dragged() {
        let f = ((table_h + resp.drag_delta().y) / full.height().max(1.0)).clamp(0.15, 0.8);
        ui.data_mut(|d| d.insert_persisted(split_id, f));
    }
    let Some(k) = v.selected() else { return go };
    let rows = &v.source.rows;
    let tag = rows.text(k, "Tag");
    let words = [rows.text(k, "Type"), rows.text(k, "Length")].into_iter().filter(|w| !w.is_empty() && w != "?").collect::<Vec<_>>().join(" \u{b7} ");
    ui.add_space(space::M);
    ui.horizontal(|ui| {
        ui.add_space(space::L);
        ui.label(egui::RichText::new(&tag).font(ecad_widgets::fonts::font(ui.ctx(), ui_type::TITLE)).color(ecad_widgets::color(s.accent_text)));
        ui.label(egui::RichText::new(&words).size(ui_type::BODY.size).color(ecad_widgets::color(s.text_2)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(space::L);
            if let Some(target) = v.gos.get(k).copied().flatten()
                && ui.add(ecad_widgets::prim::Button::new("Go to its page").small()).clicked()
            {
                go = Some(target);
            }
            if ui.add(ecad_widgets::prim::Button::new("Swap ends").small()).clicked() {
                v.swapped = !v.swapped;
            }
            ecad_widgets::prim::segmented(ui, &mut v.view, &VIEWS);
        });
    });
    ui.add_space(space::S);
    // Brief 057 (board 2.06): the cable drawn end to end, unless Cores only.
    if v.view == 0 {
        let d = v.drawing(k);
        crate::cables::view::show(ui, &d, v.swapped);
    }
    ui.add_space(space::M);
    let cable = v.ids.get(k).copied();
    // Owner rule (board 2.06): a core is open unless ticked Not used; the tick is one `setprop`.
    if let (Some((label, on)), Some(cable)) = (v.show_cores(ui), cable) {
        let key = ecad_electrical::styles::cable::unused_key(&label);
        let value = if on { serde_json::json!("yes") } else { serde_json::Value::Null };
        let _ = t.run_value("setprop", serde_json::json!({"cable": cable.hex(), "key": key, "value": value}));
    }
    go
}

/// Brief 059: a cable not drawn as + Cable's menu lists it: `-W03  H07RN-F 4G1.5 · +CAB1 → +FIELD`.
pub(crate) fn undrawn_line(r: &ecad_electrical::reports::CableRow) -> String {
    let dash = crate::reports::report::DASH;
    let ends = format!("{} \u{2192} {}", r.planned[0].as_deref().unwrap_or(dash), r.planned[1].as_deref().unwrap_or(dash));
    let tag = r.tag.clone().unwrap_or_default();
    match &r.kind {
        Some(k) => format!("{tag}  {k} \u{b7} {ends}"),
        None => format!("{tag}  {ends}"),
    }
}

/// A cable's core size: its cable type's cross-section, else its type notation's (`4G1.5` → 1.5).
fn cable_size(doc: &ecad_model::Document, k: CableId) -> Option<String> {
    use ecad_electrical::styles::{self, cable};
    let from_type = cable::of(doc, k)
        .and_then(|id| styles::field_of(doc, ecad_model::StyleCatalogue::Cable, &id, "size", None))
        .and_then(|r| ecad_electrical::devices::ratings::parse::cross_section(&r.text()));
    from_type
        .or_else(|| doc.cables.get(&k).and_then(|x| x.properties.get("type")).and_then(|t| ecad_electrical::terminal_strips::cores::notation_size(t)))
        .map(ecad_electrical::devices::ratings::dataset::fmt_num)
}

/// The wire beyond point `at` (`-M01:U1`): the number of a wire there that is not one of cable
/// `k`'s cores, else empty.
fn beyond(doc: &ecad_model::Document, k: CableId, at: &str) -> String {
    let Some((tag, label)) = at.trim().rsplit_once(':') else { return String::new() };
    let members: Vec<ecad_model::WireId> = doc.cables.get(&k).map(|c| c.cores.iter().filter_map(|m| m.wire()).collect()).unwrap_or_default();
    for c in doc.components_tagged(tag) {
        for (t, x) in doc.connection_points_of(c) {
            if x.label.as_deref() != Some(label) {
                continue;
            }
            for (w, _) in doc.ends_at(t) {
                if members.contains(&w) {
                    continue;
                }
                let n = ecad_model::connectivity::wiring::wire_number(doc, w).unwrap_or_default();
                // The wire on the far side, also when it shares the core's number (one point,
                // one net).
                if !n.is_empty() {
                    return n;
                }
            }
        }
    }
    String::new()
}
