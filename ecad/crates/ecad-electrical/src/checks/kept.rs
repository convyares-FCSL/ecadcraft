//! The kept Checks list (see [`super`]'s update map). The rules that measure text (a table over
//! the title block) take the sheet font metrics, [`TextMetrics`]; ecad-cadcraft's
//! `crate::checks::Checks` wraps this with today's signatures, passing CADCraft's fonts.

use std::collections::BTreeSet;

use ecad_model::numbering::Places;
use ecad_model::{Document, NetKey, Op, PageId};

use super::*;
use crate::text_style::fonts::TextMetrics;

/// The kept Checks list (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct Checks {
    by_key: BTreeMap<Key, Vec<Check>>,
    grid: Grid,
    /// Nets with conductors and no number (for the one Unnumbered row).
    unnumbered: BTreeSet<NetKey>,
    /// Provisional tags are on (brief 026): the Unnumbered row is a warning, `… provisional`.
    provisional: bool,
    /// Bumped on every change, for views that copy the rows.
    pub revision: u64,
    /// Rows by severity (errors, warnings, info), kept by [`Checks::set`] (brief 027's badge reads
    /// them; nothing recounts the rows).
    counts: [usize; 3],
    /// Bumped only when some row actually changed (an update that re-checked subjects and found
    /// the same rows leaves it), for views that show the rows themselves (the Release dialog).
    pub changed: u64,
    /// Brief 054: the pages whose type has checks and references off.
    off: BTreeSet<PageId>,
    /// Brief 057: the kept rating results and their indexes (design 057 §10).
    pub rating: crate::rating::Kept,
}

impl Checks {
    /// The whole list, computed from scratch (open, reload, first show).
    pub fn build_with(doc: &Document, tol: f64, m: &dyn TextMetrics) -> Self {
        let mut ch = Checks { off: ecad_model::sheet::checks_off_pages(doc), ..Checks::default() };
        for r in doc.routes.keys() {
            ch.grid.insert(doc, *r);
        }
        let conn = doc.connectivity();
        for (id, net) in conn.nets() {
            ch.set(Key::Net(id), net_checks(doc, id, net));
            if !net.wires.is_empty() && net.numbers.is_empty() {
                ch.unnumbered.insert(id);
            }
        }
        for c in doc.wires.keys() {
            let v = conductor_checks(doc, *c);
            ch.set(Key::Wire(*c), v);
        }
        for r in doc.routes.keys() {
            let v = route_checks(doc, *r, &ch.grid, tol);
            ch.set(Key::Route(*r), v);
        }
        for t in doc.terminals.keys() {
            ch.set(Key::Terminal(*t), terminal_checks(doc, *t));
        }
        // One drawing order for the pass: the book is built at most once (review of #105, 1).
        let mut places = Places::new(doc);
        for c in doc.devices.keys() {
            ch.set(Key::Device(*c), component_checks_in(&mut places, *c, m));
        }
        for k in doc.cables.keys() {
            ch.set(Key::CableReferences(*k), reference_checks(doc, *k));
            ch.set(Key::Cable(*k), cable_checks_in(&mut places, *k));
        }
        for page in doc.pages.keys() {
            ch.set(Key::Page(*page), page_checks(doc, *page));
        }
        for s in PROJECT_SOURCES {
            ch.set(s.key(), s.checks(doc));
        }
        // Brief 057.
        ch.rating = crate::rating::Kept::build(doc);
        let rows: Vec<(crate::rating::RatedSubject, Vec<Check>)> =
            ch.rating.results.values().map(|r| (r.subject, crate::rating::rows(doc, r))).collect();
        for (s, v) in rows {
            ch.set(Key::Rating(s), v);
        }
        ch.set(Key::RatingSummary, crate::rating::summary_rows(doc, &ch.rating));
        ch.provisional = ecad_model::numbering::provisional_on(doc);
        ch.set_unnumbered();
        ch
    }

    /// Brief 054b: work out the cover's rows again (after a change the cover's map names).
    pub fn set_cover(&mut self, doc: &Document) {
        self.set(Key::Cover, crate::front::CoverRows.checks(doc));
        self.revision += 1;
    }

    fn set(&mut self, key: Key, mut v: Vec<Check>) {
        // Brief 054 (design 054 §3.4): no electrical row on a page with checks and references off.
        if !self.off.is_empty() && !PROJECT_SOURCES.iter().any(|s| s.key() == key && s.kept_on_pages_off()) {
            v.retain(|c| c.page.is_none_or(|p| !self.off.contains(&p)));
        }
        let same = match self.by_key.get(&key) {
            Some(old) => *old == v,
            None => v.is_empty(),
        };
        if same {
            return;
        }
        for c in &v {
            if let Some(n) = self.counts.get_mut(c.severity() as usize) {
                *n += 1;
            }
        }
        let old = if v.is_empty() { self.by_key.remove(&key) } else { self.by_key.insert(key, v) };
        for c in old.iter().flatten() {
            if let Some(n) = self.counts.get_mut(c.severity() as usize) {
                *n = n.saturating_sub(1);
            }
        }
        self.changed += 1;
    }

    /// Rows by severity: (errors, warnings, info). O(1): kept by every change.
    pub fn counts(&self) -> (usize, usize, usize) {
        (self.counts[0], self.counts[1], self.counts[2])
    }

    fn set_unnumbered(&mut self) {
        let n = self.unnumbered.len();
        let row = (n > 0).then(|| {
            if self.provisional {
                Check {
                    kind: Kind::ProvisionalWires,
                    page: None,
                    at: None,
                    object: Object::None,
                    which: if n == 1 { "1 wire number provisional".into() } else { format!("{n} wire numbers provisional") },
                    fix: Some(Fix::Finalise),
                }
            } else {
                Check {
                    kind: Kind::Unnumbered,
                    page: None,
                    at: None,
                    object: Object::None,
                    which: if n == 1 { "1 net has no number".into() } else { format!("{n} nets have no number") },
                    fix: Some(Fix::NumberThem),
                }
            }
        });
        self.set(Key::Project, row.into_iter().collect());
    }

    /// Every row, in no particular order.
    pub fn rows(&self) -> impl Iterator<Item = &Check> {
        self.by_key.values().flatten()
    }

    /// How many rows.
    pub fn len(&self) -> usize {
        self.by_key.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    /// The rows sorted in book order (design 022 §8).
    pub fn sorted(&self, doc: &Document) -> Vec<Check> {
        sort(doc, self.rows().cloned().collect())
    }

    /// What `ops` (about to be committed on `before`) could change, worked out on the document
    /// before them: the subjects to re-check after them. Pair with [`Checks::update_with`].
    pub fn scope(&self, before: &Document, ops: &[Op]) -> Scope {
        let mut s = Scope::default();
        s.collect(before, ops, &self.grid);
        s.checks_pages(before, ops);
        s
    }

    /// Re-check the subjects in `scope` (from [`Checks::scope`]) on `doc`, the state after the ops,
    /// adding what the ops themselves name in this state.
    pub fn update_with(&mut self, doc: &Document, ops: &[Op], mut scope: Scope, tol: f64, m: &dyn TextMetrics) {
        scope.collect(doc, ops, &self.grid);
        scope.checks_pages(doc, ops);
        if scope.all {
            *self = Checks { revision: self.revision + 1, changed: self.changed + 1, ..Checks::build_with(doc, tol, m) };
            return;
        }
        // Geometry first: the grid must hold the new routes before any route is re-checked.
        let mut routes = scope.routes.clone();
        for r in &scope.routes {
            let old = self.grid.cells(*r);
            self.grid.insert(doc, *r);
            let new = self.grid.cells(*r);
            let near: BTreeSet<Cell> = old.union(&new).copied().collect();
            routes.extend(self.grid.loose_near(&near));
        }
        let conn = doc.connectivity();
        // Nets: those named, and those sharing their numbers.
        let mut nets: BTreeSet<NetKey> = BTreeSet::new();
        let mut numbers = std::mem::take(&mut scope.numbers);
        for n in &scope.nodes {
            if let Some(id) = conn.net_of(n) {
                nets.insert(id);
            }
        }
        for id in &nets {
            if let Some(net) = conn.net(*id) {
                numbers.extend(net.numbers.keys().cloned());
            }
        }
        // An unnumbered potential's shared number (`PE`) is never a shared-number error, so the
        // other nets carrying it have nothing to re-check (otherwise every PE edit would re-check
        // every PE net in the project).
        let exempt = |n: &str| doc.settings.wiring.potentials.iter().any(|p| !p.numbered && p.prefix == n);
        for n in numbers.iter().filter(|n| !exempt(n)) {
            nets.extend(conn.nets_numbered(n).map(|(id, _)| id));
        }
        // Nets that no longer exist lose their rows; those that do are checked again.
        for id in scope.old_nets.iter().chain(nets.iter()) {
            match conn.net(*id) {
                Some(net) => {
                    self.set(Key::Net(*id), net_checks(doc, *id, net));
                    if !net.wires.is_empty() && net.numbers.is_empty() {
                        self.unnumbered.insert(*id);
                    } else {
                        self.unnumbered.remove(id);
                    }
                }
                None => {
                    self.set(Key::Net(*id), Vec::new());
                    self.unnumbered.remove(id);
                }
            }
        }
        for c in &scope.wires {
            let v = conductor_checks(doc, *c);
            self.set(Key::Wire(*c), v);
        }
        for r in &routes {
            let v = route_checks(doc, *r, &self.grid, tol);
            self.set(Key::Route(*r), v);
        }
        for t in &scope.terminals {
            self.set(Key::Terminal(*t), terminal_checks(doc, *t));
        }
        let mut places = Places::new(doc);
        for c in &scope.devices {
            self.set(Key::Device(*c), component_checks_in(&mut places, *c, m));
        }
        for k in &scope.cable_references {
            self.set(Key::CableReferences(*k), reference_checks(doc, *k));
        }
        for k in &scope.cables {
            self.set(Key::Cable(*k), cable_checks_in(&mut places, *k));
        }
        // Brief 040: the dash budget of the pages the re-checked wires are on, and of the pages
        // that had a row (a wire removed or recoloured there).
        let mut pages: BTreeSet<PageId> = self.by_key.keys().filter_map(|k| if let Key::Page(p) = k { Some(*p) } else { None }).collect();
        for c in &scope.wires {
            pages.extend(crate::wires::wire_pages(doc, *c));
        }
        pages.extend(routes.iter().filter_map(|r| doc.routes.get(r)).map(|r| r.page));
        for page in pages {
            let rows = if doc.pages.contains_key(&page) { page_checks(doc, page) } else { Vec::new() };
            self.set(Key::Page(page), rows);
        }
        // Brief 057 (design 057 §10): the subjects the ops reach, and the count rows.
        let touched = self.rating.update(doc, &scope.rating);
        if !touched.is_empty() || scope.rating.any() {
            for s in touched {
                let rows = self.rating.results.get(&s).map(|r| crate::rating::rows(doc, r)).unwrap_or_default();
                self.set(Key::Rating(s), rows);
            }
            self.set(Key::RatingSummary, crate::rating::summary_rows(doc, &self.rating));
        }
        for s in PROJECT_SOURCES {
            if s.touched(&scope) {
                self.set(s.key(), s.checks(doc));
            }
        }
        self.provisional = ecad_model::numbering::provisional_on(doc);
        self.set_unnumbered();
        self.revision += 1;
    }
}
