//! Project ▸ PLC ▸ Compare (brief 056, design 056 §6.5) and the import preview (§6.7): the table
//! area's other column set, ECADCraft's side and the PLC project's side with the address between.
//! Not a dialog: the strip stays; **Back to the list** or `Esc` returns to the channels.
//!
//! **Update map.** Compare's rows come from the kept sync index (`Plc::sync`) and are made again
//! only when the PLC model updated (a take, an import, a tag edit), the scope or the filter
//! changed. The import preview holds the rows it was made with; it changes nothing until Import.

use ecad_cadcraft::plc::sync::{CompareRow, State};
use ecad_model::ComponentId;
use ecad_ui::prim::Tone;
use ecad_ui::table::{Cell, Changed, Column, ColumnId, Table, TableEvent, TableSource, Width};
use serde_json::json;

use super::ListAction;

pub const TAG: ColumnId = ColumnId(0);
pub const TEXT: ColumnId = ColumnId(1);
pub const TYPE: ColumnId = ColumnId(2);
pub const ADDRESS: ColumnId = ColumnId(3);
pub const NAME: ColumnId = ColumnId(4);
pub const COMMENT: ColumnId = ColumnId(5);
pub const PLC_TYPE: ColumnId = ColumnId(6);
pub const TAKE: ColumnId = ColumnId(7);
pub const STATE: ColumnId = ColumnId(8);
pub const CARD: ColumnId = ColumnId(9);

/// Element names (design 056 §6.5).
pub const VIEW: &str = "ecad.plc.compare.view";
/// The take button's words.
pub const USE_PLCS: &str = "Use PLC's";
pub const USE_PLCS_ADDRESS: &str = "Use PLC's address";
/// The footer's line: ECAD's own changes go out by export.
pub const FOOT: &str = "Where ECADCraft is right: Export tags, import them in the PLC project, then Import again.";
/// Writes off.
pub const WRITES_OFF: &str = "Writes are off.";

/// The columns.
pub fn columns(all: bool) -> Vec<Column> {
    let mut v = Vec::new();
    if all {
        v.push(Column::read(CARD.0, "Card", Width::Initial(56.0)));
    }
    v.extend([
        Column::read(TAG.0, "Tag", Width::Initial(130.0)),
        Column::read(TEXT.0, "Description", Width::Initial(180.0)),
        Column::read(TYPE.0, "Type", Width::Initial(44.0)),
        Column::read(ADDRESS.0, "Address", Width::Initial(80.0)),
        Column::read(NAME.0, "PLC name", Width::Initial(130.0)),
        Column::read(COMMENT.0, "Comment", Width::Initial(180.0)),
        Column::read(PLC_TYPE.0, "PLC type", Width::Initial(64.0)),
        Column::read(TAKE.0, "Take", Width::Initial(140.0)).unsorted(),
        Column::read(STATE.0, "State", Width::Remainder),
    ]);
    v
}

/// The rows as a table source.
#[derive(Debug, Default)]
pub struct CompareSource {
    cols: Vec<Column>,
    keys: Vec<usize>,
    pub rows: Vec<CompareRow>,
    /// Take buttons are offered.
    pub writable: bool,
    /// The import preview shows no take buttons (Import takes them all, or none).
    pub preview: bool,
}

impl CompareSource {
    fn set(&mut self, rows: Vec<CompareRow>, all: bool) {
        self.cols = columns(all);
        self.keys = (0..rows.len()).collect();
        self.rows = rows;
    }
}

/// The State column's words for a row with no channel (a tag on no card).
fn state_words(r: &CompareRow) -> (String, Tone) {
    if r.at.is_none() {
        return (format!("no card at {}", r.address), Tone::Neutral);
    }
    let w = match &r.state {
        State::Differs(_) => "differs",
        State::Moved(_) => "moved",
        State::NotInPlc => "not in PLC",
        State::OnlyInPlc(_) => "only in PLC",
        State::InSync => "in sync",
        State::NoAddress => "no address",
        State::Nothing | State::Free => "",
    };
    let tone = match &r.state {
        State::InSync => Tone::Ok,
        State::NotInPlc => Tone::Error,
        s if s.counted() => Tone::Warning,
        _ => Tone::Neutral,
    };
    (w.to_string(), tone)
}

impl TableSource for CompareSource {
    type Key = usize;

    fn columns(&self) -> &[Column] {
        &self.cols
    }

    fn keys(&self) -> &[usize] {
        &self.keys
    }

    fn contains(&self, key: usize) -> bool {
        key < self.rows.len()
    }

    fn cell(&self, key: usize, col: ColumnId) -> Cell {
        let Some(r) = self.rows.get(key) else { return Cell::Empty };
        let dash = || Cell::read("\u{2014}");
        let differs = matches!(r.state, State::Differs(_) | State::OnlyInPlc(_));
        match col {
            CARD => Cell::read(r.card_tag.clone()),
            TAG if r.at.is_none() => Cell::Empty,
            TAG => match &r.tag {
                Some(t) if differs => Cell::Badge(t.clone(), Tone::Warning),
                Some(t) => Cell::read(t.clone()),
                None => dash(),
            },
            TEXT => Cell::read(r.text.clone().unwrap_or_default()),
            TYPE => Cell::read(r.ty),
            ADDRESS => Cell::read(r.address.clone()),
            NAME => match &r.plc {
                Some(p) if differs => Cell::Badge(p.name.clone(), Tone::Warning),
                Some(p) => Cell::read(p.name.clone()),
                None => dash(),
            },
            COMMENT => Cell::read(r.plc.as_ref().map(|p| p.comment.clone()).unwrap_or_default()),
            PLC_TYPE => Cell::read(r.plc.as_ref().map(|p| p.data_type.clone()).unwrap_or_default()),
            TAKE if self.preview || !self.writable || r.at.is_none() => Cell::Empty,
            TAKE => match &r.state {
                State::Differs(_) | State::OnlyInPlc(_) => Cell::Buttons(vec![USE_PLCS.into()]),
                State::Moved(_) => Cell::Buttons(vec![USE_PLCS_ADDRESS.into()]),
                _ => Cell::Empty,
            },
            STATE => {
                let (w, tone) = state_words(r);
                if w.is_empty() { Cell::Empty } else { Cell::Badge(w, tone) }
            }
            _ => Cell::Empty,
        }
    }

    fn row_label(&self, key: usize) -> String {
        self.rows.get(key).map_or_else(String::new, |r| match r.channel {
            Some(ch) => format!("{} channel {ch}", r.card_tag),
            None => format!("PLC tag at {}", r.address),
        })
    }
}

/// An import waiting for its answer (design 056 §6.7 step 2).
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    /// The file chosen.
    pub path: String,
    /// `412 tags read; 96 with an I/O address; …`.
    pub summary: String,
    /// **Also take the PLC's names**.
    pub take: bool,
}

/// Compare, or the import preview, in place of the channel table.
#[derive(Debug)]
pub struct CompareView {
    pub source: CompareSource,
    pub table: Table<usize>,
    /// `Compare -A03 with P56-tags.csv (csv), imported 2026-11-14 15:02`.
    pub title: String,
    /// The card compared (`This card`), or every card.
    pub card: Option<ComponentId>,
    pub all_cards: bool,
    pub only_differences: bool,
    /// The import preview's answer, when this is one.
    pub pending: Option<Pending>,
    /// The PLC model's update count the rows were made at.
    pub revision: Option<u64>,
    /// Rows made in all (tests: the update map).
    pub builds: u64,
}

impl CompareView {
    /// Compare for `card` (every card when `None`).
    pub fn new(card: Option<ComponentId>) -> Self {
        let mut table = Table::new("ecad.plc.compare", "Compare");
        table.toolbar = false;
        table.empty = "No differences.";
        Self {
            source: CompareSource::default(),
            table,
            title: String::new(),
            card,
            all_cards: card.is_none(),
            only_differences: true,
            pending: None,
            revision: None,
            builds: 0,
        }
    }

    /// The import preview of `path` with its rows and summary.
    pub fn preview(path: String, summary: String, rows: Vec<CompareRow>, source: &str) -> Self {
        let mut v = Self::new(None);
        v.title = format!("Import {source}: preview, not written yet");
        v.pending = Some(Pending { path, summary, take: false });
        v.source.preview = true;
        v.set_rows(rows);
        v
    }

    /// Show `rows`.
    pub fn set_rows(&mut self, rows: Vec<CompareRow>) {
        self.source.set(rows, self.all_cards || self.card.is_none());
        self.table.reset();
        self.table.update(&self.source, Changed::All);
        self.builds += 1;
    }

    /// Make the rows again from the kept model when it moved on (not for a preview).
    pub fn sync(&mut self, doc: &ecad_model::Document, plc: &ecad_cadcraft::plc::Plc, writable: bool) {
        if self.pending.is_some() {
            return;
        }
        let w = self.source.writable != writable;
        self.source.writable = writable;
        if self.revision == Some(plc.updates) && !w {
            return;
        }
        self.revision = Some(plc.updates);
        let cards: Option<Vec<ComponentId>> = if self.all_cards { None } else { self.card.map(|c| vec![c]) };
        let rows = ecad_cadcraft::plc::sync::compare_rows(doc, plc, &plc.sync, cards.as_deref(), !self.only_differences);
        let who = if self.all_cards {
            "the project".to_string()
        } else {
            self.card.and_then(|c| plc.cards.get(&c)).map_or_else(String::new, |c| c.tag.clone())
        };
        self.title = match &plc.sync.snapshot {
            Some(s) => {
                let when =
                    s.at.as_deref()
                        .and_then(ecad_cadcraft::plc::sync::at_ms)
                        .map(|ms| ecad_cadcraft::history::full_time(Some(ms)))
                        .unwrap_or_default();
                format!("Compare {who} with {}, imported {when}", s.source.as_deref().unwrap_or("a tag table"))
            }
            None => format!("Compare {who}: no tag table imported yet"),
        };
        self.set_rows(rows);
    }

    /// Change the scope or the filter: the rows are made again at the next sync.
    fn invalidate(&mut self) {
        self.revision = None;
    }

    /// Draw the head, the table and the foot. Returns what it asks for; `back` is set when the
    /// user leaves.
    pub fn show(&mut self, ui: &mut eframe::egui::Ui, back: &mut bool) -> Vec<ListAction> {
        use ecad_ui::tokens::{space, ui_type};
        use eframe::egui;
        let mut out = Vec::new();
        let s = ecad_ui::SHELL;
        ui.push_id(VIEW, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(space::M);
                let title = ecad_ui::fonts::font(ui.ctx(), ui_type::TITLE);
                ui.label(egui::RichText::new(&self.title).font(title).color(ecad_ui::color(s.text)));
                if self.pending.is_none() {
                    ui.add_space(space::L);
                    let mut i = usize::from(self.all_cards);
                    ui.push_id("scope", |ui| ecad_ui::prim::segmented(ui, &mut i, &[super::plc::THIS_CARD, super::plc::ALL_CARDS]));
                    if (i == 1) != self.all_cards && (self.card.is_some() || i == 1) {
                        self.all_cards = i == 1;
                        self.invalidate();
                    }
                    let mut only = self.only_differences;
                    if ui.checkbox(&mut only, "Only differences").changed() {
                        self.only_differences = only;
                        self.invalidate();
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(space::M);
                    if ecad_ui::prim::button(ui, "Back to the list", ecad_ui::prim::Kind::Quiet).clicked() {
                        *back = true;
                    }
                });
            });
            if let Some(p) = &mut self.pending {
                ui.horizontal(|ui| {
                    ui.add_space(space::M);
                    ui.label(egui::RichText::new(&p.summary).size(ui_type::SMALL.size).color(ecad_ui::color(s.text_2)));
                });
                ui.horizontal(|ui| {
                    ui.add_space(space::M);
                    ui.radio_value(&mut p.take, false, "Keep the snapshot only");
                    ui.radio_value(&mut p.take, true, "Also take the PLC's names");
                });
            }
            ui.add_space(space::S);
            // As tall as its rows, so the footer's buttons sit under them.
            let rows = self.source.keys().len().max(1) as f32 + 1.0;
            let h = (rows * 30.0 + space::M).min((ui.available_height() - 60.0).max(60.0));
            let events = ui
                .allocate_ui(egui::vec2(ui.available_width(), h), |ui| {
                    ui.set_max_height(h);
                    self.table.show(ui, &self.source)
                })
                .inner;
            for e in events {
                if let TableEvent::Button { key, .. } = e
                    && let Some(r) = self.source.rows.get(key)
                    && let Some((c, _)) = r.at
                    && let Some(ch) = r.channel
                {
                    let what = if matches!(r.state, State::Moved(_)) { "address" } else { "name" };
                    out.push(ListAction::Run(format!("plctake {}", json!({"items": [{"card": c.hex(), "channel": ch}], "what": what}))));
                }
            }
            ui.add_space(space::S);
            ui.horizontal(|ui| {
                ui.add_space(space::M);
                match self.pending.clone() {
                    Some(p) => {
                        let take = if p.take { "names" } else { "none" };
                        if ecad_ui::prim::button(ui, "Import", ecad_ui::prim::Kind::Primary).clicked() {
                            out.push(ListAction::Run(format!("plcimport {}", json!({"path": p.path, "take": take}))));
                            *back = true;
                        }
                        if ecad_ui::prim::button(ui, "Cancel", ecad_ui::prim::Kind::Secondary).clicked() {
                            *back = true;
                        }
                    }
                    None => {
                        let n = self.source.rows.iter().filter(|r| matches!(r.state, State::Differs(_) | State::OnlyInPlc(_))).count();
                        if n > 0 {
                            let label = format!("Use the PLC's name for all {n}");
                            let r =
                                ui.add_enabled_ui(self.source.writable, |ui| ecad_ui::prim::button(ui, &label, ecad_ui::prim::Kind::Secondary)).inner;
                            if r.on_disabled_hover_text(WRITES_OFF).clicked() {
                                let p = match (self.all_cards, self.card) {
                                    (false, Some(c)) => json!({"card": c.hex()}),
                                    _ => json!({"all": true}),
                                };
                                out.push(ListAction::Run(format!("plctake {p}")));
                            }
                        }
                        if ecad_ui::prim::button(ui, "Export tags", ecad_ui::prim::Kind::Secondary).clicked() {
                            out.push(ListAction::ExportTags);
                        }
                        if ecad_ui::prim::button(ui, "Import again", ecad_ui::prim::Kind::Secondary).clicked() {
                            out.push(ListAction::ImportTags);
                        }
                        ui.label(egui::RichText::new(FOOT).size(ui_type::SMALL.size).color(ecad_ui::color(s.text_3)));
                    }
                }
            });
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            *back = true;
        }
        out
    }
}
