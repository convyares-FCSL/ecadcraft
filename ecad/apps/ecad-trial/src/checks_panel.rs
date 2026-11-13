//! The **Checks** tab of the Project panel (brief 022, design 022 §8): open ends, loose wires,
//! false-looking connections, nets with one connection point, unconnected device points and
//! number or potential conflicts, each row with where it is and a go-to.
//!
//! The rows are a copy of [`ecad_cadcraft::checks::Checks`], which the project builds whole on the
//! tab's first show and then keeps from each transaction's ops; the copy is taken again only when
//! that list's revision moved (or the project or the Show filter changed). Live: no refresh button.
//! Brief 039: the rows are drawn by the one table ([`crate::lists::checks`]), patched with the rows
//! that appeared or went; this tab is its host until the shell (U2) places it.

use std::collections::BTreeSet;

use ecad_cadcraft::checks::{Check, Fix, Kind, Severity};
use eframe::egui::{self, RichText};

use crate::project_panel::Action;

/// Accessible label of the tab.
pub const CHECKS_TAB: &str = "Checks";
/// What an empty list says.
pub const NO_PROBLEMS: &str = "No problems found.";
/// The fix buttons.
pub const ADD_JUNCTION: &str = "Add junction";
/// Brief 032: the Looks connected row's fix (`joinwires {at}`).
pub const JOIN: &str = "Join";
pub const NUMBER_THEM: &str = "Number them";
/// Brief 026: opens Finalise numbering (the Renumber dialog), never a direct write.
pub const FINALISE: &str = "Finalise";
/// Brief 024's fix buttons.
pub const USE_BOX_LOCATION: &str = "Use the box's location";
pub const SHOW: &str = "Show";
/// Brief 028's fix buttons.
pub const OFFER: &str = "Offer drawn";
pub const RENUMBER_STRIP: &str = "Renumber strip";
pub const SET_CORES: &str = "Set cores\u{2026}";
/// Brief 044: `connector-over`'s fix.
pub const SET_PINS: &str = "Set pins";
pub const NEXT_FREE_CORE: &str = "Next free core";
/// Brief 030's fix buttons, two per row (design 030 §8).
pub const MOVE_TO_END: &str = "Move to end";
pub const REMOVE: &str = "Remove";
/// Brief 033: start Move Mirror Table for a table over the title block.
pub const MOVE_TABLE: &str = "Move table…";
/// Brief 033: open a page's number field (a continuation sheet took its number).
pub const RENUMBER_PAGE: &str = "Renumber…";
pub const SPLIT_LINK: &str = "Split link";
pub const REMOVE_PARTITION: &str = "Remove partition";
pub const DROP_MISSING: &str = "Drop missing";
pub const DELETE_LINK: &str = "Delete link";
/// Brief 036's fix buttons (design 036 §8).
pub const MOVE_NEXT_FREE: &str = "Move to next free";
pub const MOVE_NEW_POSITION: &str = "Move to new position";
pub const DROP_MISSING_LEVELS: &str = "Drop missing levels";
pub const MAKE_STRIP_POSITION: &str = "Make strip position";

/// The two buttons of a brief 030 fix.
pub fn pair(f: Fix) -> Option<[&'static str; 2]> {
    match f {
        Fix::AccessoryOrphan(..) => Some([MOVE_TO_END, REMOVE]),
        Fix::LinkAcrossPartition(..) => Some([SPLIT_LINK, REMOVE_PARTITION]),
        Fix::LinkOrphan(..) => Some([DROP_MISSING, DELETE_LINK]),
        Fix::LevelMissing(..) => Some([DROP_MISSING_LEVELS, DELETE_LINK]),
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
    /// Brief 039: the rows as a table source, and the table.
    pub source: crate::lists::checks::ChecksSource,
    pub table: ecad_ui::table::Table<crate::lists::checks::CheckKey>,
    /// What the last copy did to the table (tests).
    pub last_change: Option<ecad_ui::table::Changed<crate::lists::checks::CheckKey>>,
}

impl Default for ChecksPanel {
    fn default() -> Self {
        let mut table = ecad_ui::table::Table::new("ecad.checks", "Checks");
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

    /// Errors and warnings among the rows shown (the shell's Checks pill, brief 038).
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
    pub fn sync(&mut self, serial: u64, p: &mut ecad_cadcraft::Project) {
        let hidden = self.hidden.clone();
        // Brief 024: the location rows come from the report models, kept the same way.
        let revision = p.checks().revision.wrapping_mul(1_000_003).wrapping_add(p.outputs().revision);
        if self.seen.as_ref().is_some_and(|(s, r, h)| *s == serial && *r == revision && *h == hidden) {
            return;
        }
        // Built now if it was not; read without changing anything from here on.
        let p: &ecad_cadcraft::Project = p;
        let doc = p.ledger.document();
        let Some(list) = p.checks.as_ref() else { return };
        let mut all: Vec<Check> = list.rows().cloned().collect();
        all.extend(p.outputs.as_ref().map(|o| o.checks()).unwrap_or_default());
        let rows: Vec<Row> = ecad_cadcraft::checks::sort(doc, all)
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
        let changed = if same_view { changed } else { ecad_ui::table::Changed::All };
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
    fn fix(r: &Row, f: Fix, k: usize) -> Option<Action> {
        if pair(f).is_some() {
            return Some(Action::AccessoryFix(f, k));
        }
        Some(match (f, r.check.page) {
            (Fix::AddJunction(at), Some(page)) => Action::AddJunction(page, at),
            (Fix::Join(at), Some(page)) => Action::JoinWires(page, at),
            (Fix::NumberThem, _) => Action::NumberThem,
            (Fix::Finalise, _) => Action::Finalise,
            (Fix::UseBoxLocation(c), _) => Action::UseBoxLocation(c),
            (Fix::ShowReports, _) => Action::ShowReports,
            (Fix::OfferDrawn(c), _) => Action::OfferDrawn(c),
            (Fix::RenumberStrip(c), _) => Action::RenumberStrip(c),
            (Fix::SetCores(k), _) => Action::SetCores(k),
            (Fix::NextFreeCore(w), _) => Action::NextFreeCore(w),
            (Fix::ShowCables, _) => Action::ShowCables,
            (
                Fix::LinkOnNonStrip(_)
                | Fix::MoveToNextFree(_)
                | Fix::MoveToNewPosition(_)
                | Fix::FuseDevices
                | Fix::ConnectorNextPin(_)
                | Fix::SetPins(..)
                | Fix::AccessoryOrphan(..)
                | Fix::LinkAcrossPartition(..)
                | Fix::LinkOrphan(..)
                | Fix::LevelMissing(..),
                _,
            ) => Action::AccessoryFix(f, 0),
            (Fix::MoveTable(_), _) => Action::MoveTable(Box::new(r.check.clone())),
            (Fix::RenumberPage(page), _) => Action::RenumberPage(page),
            (Fix::AddJunction(_) | Fix::Join(_), None) => return None,
        })
    }

    /// The tab's contents: the Show filter and counts, then the table. Returns the actions asked
    /// for.
    pub fn show(&mut self, ui: &mut egui::Ui, writable: bool) -> Vec<Action> {
        use ecad_ui::table::TableEvent;
        let mut out = Vec::new();
        let dim = ecad_ui::color(ecad_ui::SHELL.text_2);
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
            self.table.update(&self.source, ecad_ui::table::Changed::All);
        }
        let events = self.table.show(ui, &self.source);
        self.rows_drawn = self.table.rows_drawn;
        let index = |k: crate::lists::checks::CheckKey| self.source.row(k).and_then(|r| self.rows.iter().position(|x| x == r));
        for e in events {
            match e {
                // One row clicked (or reached with the arrows): show it (design 022 §8).
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
                TableEvent::Export(csv) => crate::lists::copy_csv(ui, csv),
                TableEvent::Selected(_) | TableEvent::Commit { .. } => {}
            }
        }
        out
    }
}
