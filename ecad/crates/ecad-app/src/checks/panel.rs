//! The **Checks** tab of the Project panel: open ends, loose wires,
//! false-looking connections, nets with one connection point, unconnected device points and
//! number or potential conflicts, each row with where it is and a go-to.
//!
//! The rows are a copy of [`ecad_electrical::checks::kept::Checks`], which the project builds whole on the
//! tab's first show and then keeps from each transaction's ops; the copy is taken again only when
//! that list's revision moved (or the project or the Show filter changed). Live: no refresh button.
//! The rows are drawn by the one table ([`crate::checks::list`]), patched with the rows
//! that appeared or went; this tab is its host until the shell places it.

use std::collections::BTreeSet;

use eframe::egui::{self, RichText};
use {ecad_electrical::checks::Check, ecad_electrical::checks::Fix, ecad_electrical::checks::Kind, ecad_electrical::checks::Severity};

use crate::pages::project_panel::Action;

/// What the tab says while its first build runs on the background pool.
pub(crate) const RUNNING: &str = "Running…";
/// Accessible label of the tab.
pub const CHECKS_TAB: &str = "Checks";
/// What an empty list says.
pub(crate) const NO_PROBLEMS: &str = "No problems found.";
/// The fix buttons.
/// The Looks connected row's fix (`joinwires {at}`).
pub const JOIN: &str = "Join";
pub(crate) const NUMBER_THEM: &str = "Number them";
/// Opens Finalise numbering (the Renumber dialog), never a direct write.
pub const FINALISE: &str = "Finalise";
/// Fix buttons for location and function checks.
pub(crate) const USE_CONTEXT: &str = "Use it";
pub(crate) const SET_LOCATION: &str = "Set location";
pub(crate) const SET_FUNCTION: &str = "Set function";
pub(crate) const SHOW_IN_SETTINGS: &str = "Show in Settings";
pub const SHOW: &str = "Show";
/// A style row's fix opens Settings ▸ Standards.
pub(crate) const GO_TO_STANDARDS: &str = "Go to Standards";
/// A title block row's fixes.
pub(crate) const EDIT_BLOCK: &str = "Edit block";
pub(crate) const CLEAR_PAGE_SHEET: &str = "Clear the page's sheet";
/// Fix buttons for terminal strip checks.
pub(crate) const OFFER: &str = "Offer drawn";
pub(crate) const RENUMBER_STRIP: &str = "Renumber strip";
pub(crate) const SET_CORES: &str = "Set cores\u{2026}";
/// `connector-over`'s fix.
pub(crate) const SET_PINS: &str = "Set pins";
/// `connector-lone-pin`'s fix.
pub(crate) const PLACE_MATES: &str = "Place mates";
/// `connector-family` / `connector-size`'s fix.
pub(crate) const IGNORE: &str = "Ignore";
pub(crate) const NEXT_FREE_CORE: &str = "Next free core";
/// Fix buttons for terminal strip accessory and jumper bar checks, two per row.
pub(crate) const MOVE_TO_END: &str = "Move to end";
/// An open core's two fixes.
pub(crate) const TICK_NOT_USED: &str = "Tick Not used";
pub const GO_TO_OPEN_END: &str = "Go to the open end";
pub const REMOVE: &str = "Remove";
/// Start Move Mirror Table for a table over the title block.
pub(crate) const MOVE_TABLE: &str = "Move table…";
/// Open a page's number field (a continuation sheet took its number).
pub(crate) const RENUMBER_PAGE: &str = "Renumber…";
pub(crate) const SPLIT_JUMPER_BAR: &str = "Split jumper bar";
pub(crate) const REMOVE_PARTITION: &str = "Remove partition";
pub(crate) const DROP_MISSING: &str = "Drop missing";
pub(crate) const DELETE_JUMPER_BAR: &str = "Delete jumper bar";
/// Fix buttons for multi-level terminal block checks.
pub(crate) const MOVE_NEXT_FREE: &str = "Move to next free";
pub(crate) const MOVE_NEW_POSITION: &str = "Move to new position";
pub(crate) const DROP_MISSING_LEVELS: &str = "Drop missing levels";
/// Fix button for two article numbers of one part.
pub(crate) const USE_FOR_ALL: &str = "Use for all";
pub(crate) const MAKE_STRIP_POSITION: &str = "Make strip position";
/// The PLC rows' buttons.
pub(crate) const MOVE_TO_FREE_SLOT: &str = "Move to free slot";
pub(crate) const PUT_IN_RACK: &str = "Put in rack…";
pub(crate) const TAKE_OUT_OF_RACK: &str = "Take out of rack";
pub(crate) const RAISE_SLOTS: &str = "Raise slots";

/// The two buttons of a fix that has two.
pub fn pair(f: Fix) -> Option<[&'static str; 2]> {
    match f {
        Fix::AccessoryOrphan(..) => Some([MOVE_TO_END, REMOVE]),
        Fix::JumperBarAcrossPartition(..) => Some([SPLIT_JUMPER_BAR, REMOVE_PARTITION]),
        Fix::JumperBarOrphan(..) => Some([DROP_MISSING, DELETE_JUMPER_BAR]),
        Fix::LevelMissing(..) => Some([DROP_MISSING_LEVELS, DELETE_JUMPER_BAR]),
        Fix::CoreOpen(..) => Some([TICK_NOT_USED, GO_TO_OPEN_END]),
        Fix::AssignCable(_) | Fix::UseDrawing(..) => crate::cables::manager::pair(f),
        Fix::Plc(_, ecad_electrical::plc::PlcFix::RackMissing) => Some([PUT_IN_RACK, TAKE_OUT_OF_RACK]),
        Fix::Plc(_, ecad_electrical::plc::PlcFix::SlotRange) => Some([RAISE_SLOTS, MOVE_TO_FREE_SLOT]),
        _ => None,
    }
}

/// One copied row, ready to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub check: Check,
    /// `/2.1`.
    pub locator: String,
}

/// The accessible label of a row: `Warning: Open end, /3.2, -W01 3`.
pub fn row_label(r: &Row) -> String {
    let sev = match r.check.severity() {
        Severity::Error => "Error",
        Severity::Warning => "Warning",
        Severity::Info => "Info",
    };
    format!("{sev}: {}, {}, {}", r.check.what(), r.locator, r.check.which)
}

/// The tab's state.
#[derive(Debug)]
pub struct ChecksPanel {
    rows: Vec<Row>,
    /// (project serial, list revision, filter) the copy was taken at.
    seen: Option<(u64, u64, BTreeSet<Kind>)>,
    /// Kinds unticked in Show (kept for the session).
    pub hidden: BTreeSet<Kind>,
    /// The row gone to last (highlighted).
    pub selected: Option<usize>,
    /// After a click in the list, ↑/↓ move between rows and Enter goes to.
    pub armed: bool,
    /// Counts of the shown rows: errors, warnings.
    counts: (usize, usize),
    /// How many times the copy was taken (tests: only when the list changed).
    pub copies: u64,
    /// Rows laid out on the last frame (tests: virtualised).
    pub rows_drawn: usize,
    /// The project cannot be written: no fix buttons.
    pub read_only: bool,
    /// The first build of the Checks list and the report models is still running (issue 48): the
    /// tab says so instead of the rows.
    pub building: bool,
    /// The rows as a table source, and the table.
    pub source: crate::checks::list::ChecksSource,
    pub table: ecad_widgets::table::Table<crate::checks::list::CheckKey>,
    /// What the last copy did to the table (tests).
    pub last_change: Option<ecad_widgets::table::Changed<crate::checks::list::CheckKey>>,
}

impl Default for ChecksPanel {
    fn default() -> Self {
        let mut table = ecad_widgets::table::Table::new("ecad.checks", "Checks");
        table.empty = NO_PROBLEMS;
        Self {
            rows: Vec::new(),
            seen: None,
            hidden: BTreeSet::new(),
            selected: None,
            armed: false,
            counts: (0, 0),
            copies: 0,
            rows_drawn: 0,
            read_only: false,
            building: false,
            source: Default::default(),
            table,
            last_change: None,
        }
    }
}

impl ChecksPanel {
    /// The rows shown now.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Errors and warnings among the rows shown (the shell's Checks pill).
    pub fn counts(&self) -> (usize, usize) {
        self.counts
    }

    /// The title of the tab: `Checks (5)`.
    pub fn title(&self) -> String {
        if self.rows.is_empty() { CHECKS_TAB.to_string() } else { format!("{CHECKS_TAB} ({})", self.rows.len()) }
    }

    /// Forget the copy: the next [`ChecksPanel::sync`] takes it again and the table is built
    /// whole (shown again).
    pub fn invalidate(&mut self) {
        self.seen = None;
        self.table.reset();
    }

    /// Bring the copy up to date with the project's kept list (built on the first call).
    pub fn sync(&mut self, serial: u64, p: &mut ecad_engine::Project) {
        let hidden = self.hidden.clone();
        // The location rows come from the report models, kept the same way.
        // Both are kept by now (the caller waits for the background build); never built here.
        let (Some(checks), Some(reports)) = (p.kept_checks(), p.kept_reports()) else { return };
        let revision = checks.revision.wrapping_mul(1_000_003).wrapping_add(reports.revision);
        if self.seen.as_ref().is_some_and(|(s, r, h)| *s == serial && *r == revision && *h == hidden) {
            return;
        }
        // Built now if it was not; read without changing anything from here on.
        let p: &ecad_engine::Project = p;
        let doc = p.ledger.document();
        let Some(list) = p.kept_checks() else { return };
        let mut all: Vec<Check> = list.rows().cloned().collect();
        all.extend(p.kept_reports().map(|o| o.checks()).unwrap_or_default());
        let rows: Vec<Row> = ecad_electrical::checks::sort(doc, all)
            .into_iter()
            .filter(|c| !hidden.contains(&c.kind))
            .map(|c| Row { locator: c.locator(doc), check: c })
            .collect();
        self.counts = (
            rows.iter().filter(|r| r.check.severity() == Severity::Error).count(),
            rows.iter().filter(|r| r.check.severity() == Severity::Warning).count(),
        );
        // The update map: a new project or Show filter rebuilds; otherwise only the rows that
        // appeared or went are patched.
        let same_view = self.seen.as_ref().is_some_and(|(s, _, h)| *s == serial && *h == hidden);
        let changed = self.source.set(&rows);
        let changed = if same_view { changed } else { ecad_widgets::table::Changed::All };
        self.table.update(&self.source, changed.clone());
        self.last_change = Some(changed);
        self.rows = rows;
        if self.selected.is_some_and(|i| i >= self.rows.len()) {
            self.selected = None;
        }
        self.seen = Some((serial, revision, hidden));
        self.copies += 1;
    }

    /// The go-to of row `i`.
    fn goto(&self, i: usize) -> Option<Action> {
        let r = self.rows.get(i)?;
        Some(Action::GotoCheck(Box::new(r.check.clone())))
    }

    /// Button `k` of row `r`'s fix `f`, as an action.
    pub(crate) fn fix(r: &Row, f: Fix, k: usize) -> Option<Action> {
        // An open core's Tick Not used / Go to the open end.
        if let Fix::CoreOpen(..) = f {
            return Some(Action::CoreFix(f, k));
        }
        // The cable manager's rows.
        if let Fix::AssignCable(_) | Fix::UseDrawing(..) | Fix::ShowCableField(..) = f {
            return Some(Action::CableFix(f, k));
        }
        if pair(f).is_some() {
            return Some(Action::AccessoryFix(f, k));
        }
        Some(match (f, r.check.page) {
            (Fix::Join(at), Some(page)) => Action::JoinWires(page, at),
            (Fix::NumberThem, _) => Action::NumberThem,
            (Fix::Finalise, _) => Action::Finalise,
            (Fix::UseContext(c, a), _) => Action::UseContext(c, a),
            (Fix::SetDesignation(c, _), _) => Action::GoToDevice(c),
            (Fix::ShowDesignations, _) => Action::ShowDesignations,
            (Fix::ShowReports, _) => Action::ShowReports,
            (Fix::OfferDrawn(c), _) => Action::OfferDrawn(c),
            (Fix::RenumberStrip(c), _) => Action::RenumberStrip(c),
            (Fix::SetCores(k), _) => Action::SetCores(k),
            (Fix::NextFreeCore(w), _) => Action::NextFreeCore(w),
            (Fix::ShowCables, _) => Action::ShowCables,
            (Fix::OpenStandards(w), _) => Action::OpenStandards(w),
            // Cable rating and cable type fixes.
            (
                Fix::ShowRating(_)
                | Fix::UseSize(..)
                | Fix::PickCableType(..)
                | Fix::SetBasis(..)
                | Fix::ClearProtectedBy(_)
                | Fix::ChooseRatingTable
                | Fix::ShowRatedWires
                | Fix::CoreOpen(..),
                _,
            ) => Action::RatingFix(f),
            (
                Fix::JumperBarOnNonStrip(_)
                | Fix::MoveToNextFree(_)
                | Fix::MoveToNewPosition(_)
                | Fix::FuseDevices
                | Fix::Plc(..)
                | Fix::ConnectorNextPin(_)
                | Fix::SetPins(..)
                | Fix::IgnoreConnector(..)
                | Fix::AccessoryOrphan(..)
                | Fix::JumperBarAcrossPartition(..)
                | Fix::JumperBarOrphan(..)
                | Fix::LevelMissing(..)
                | Fix::UseArticle(..)
                | Fix::DropMissingFor(..),
                _,
            ) => Action::AccessoryFix(f, 0),
            (Fix::PlaceMates(c, side), _) => Action::PlaceMates(c, side),
            (Fix::MoveTable(_), _) => Action::MoveTable(Box::new(r.check.clone())),
            (Fix::RenumberPage(page), _) => Action::RenumberPage(page),
            // Use its usual type, Reset, Remove.
            (Fix::Page(f), _) => Action::RunLine(f.line()),
            (Fix::EditTitleBlock(page, item), _) => Action::EditTitleBlock(page, item),
            (Fix::ClearPageSheet(page), _) => Action::ClearPageSheet(page),
            (Fix::Join(_), None) => return None,
            (Fix::AssignCable(_) | Fix::UseDrawing(..) | Fix::ShowCableField(..), _) => Action::CableFix(f, k),
        })
    }

    /// The tab's contents: the Show filter and counts, then the table. Returns the actions asked
    /// for.
    pub fn show(&mut self, ui: &mut egui::Ui, writable: bool) -> Vec<Action> {
        use ecad_widgets::table::TableEvent;
        let mut out = Vec::new();
        if self.building {
            ecad_widgets::prim::note(ui, RUNNING, ecad_widgets::prim::Tone::Neutral);
            return out;
        }
        let dim = ecad_widgets::color(ecad_widgets::SHELL.text_2);
        ui.horizontal(|ui| {
            let label = if self.hidden.is_empty() { "All kinds" } else { "Some kinds" };
            ui.label(RichText::new("Show:").small());
            ui.menu_button(RichText::new(format!("{label} \u{25be}")).small(), |ui| {
                for k in Kind::ALL {
                    let mut on = !self.hidden.contains(&k);
                    if ui.checkbox(&mut on, k.words()).changed() {
                        if on {
                            self.hidden.remove(&k);
                        } else {
                            self.hidden.insert(k);
                        }
                    }
                }
            });
            let (e, w) = self.counts;
            let words = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
            ui.label(RichText::new(format!("{} \u{b7} {}", words(e, "error", "errors"), words(w, "warning", "warnings"))).small().color(dim));
        });
        let fixes = writable && !self.read_only;
        if self.source.fixes != fixes {
            self.source.fixes = fixes;
            self.table.update(&self.source, ecad_widgets::table::Changed::All);
        }
        let events = self.table.show(ui, &self.source);
        self.rows_drawn = self.table.rows_drawn;
        let index = |k: crate::checks::list::CheckKey| self.source.row(k).and_then(|r| self.rows.iter().position(|x| x == r));
        for e in events {
            match e {
                // One row clicked (or reached with the arrows): show it.
                TableEvent::Selected(keys) if keys.len() == 1 => {
                    if let Some(i) = keys.first().and_then(|k| index(*k)) {
                        self.selected = Some(i);
                        self.armed = true;
                        out.extend(self.goto(i));
                    }
                }
                TableEvent::GoTo { key, .. } | TableEvent::Open(key) => {
                    if let Some(i) = index(key) {
                        self.selected = Some(i);
                        out.extend(self.goto(i));
                    }
                }
                TableEvent::Button { key, index: k, .. } => {
                    if let Some(r) = self.source.row(key)
                        && let Some(f) = r.check.fix
                    {
                        out.extend(Self::fix(r, f, k));
                    }
                }
                TableEvent::Export(csv) => crate::workspace::copy_csv(ui, csv),
                TableEvent::Selected(_) | TableEvent::Commit { .. } => {}
            }
        }
        out
    }
}
