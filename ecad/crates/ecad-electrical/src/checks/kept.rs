//! The kept Checks list (see [`super`]'s update map). The rules that measure text (a table over
//! the title block) take the text look, [`ecad_model::look::Look`]; `crate::views::ChecksView`
//! keeps the list, measuring with the look the engine supplies. The rating rows are worked out from
//! the rating results the project keeps in their own view ([`RatingView`]).

use super::cables::{cable_checks_in, reference_checks};
use super::devices::component_checks_in;
use super::grid::{Cell, Grid};
use super::nets::{aid_checks, conductor_checks, connection_point_checks, net_checks, page_checks, route_checks};
use std::collections::BTreeSet;

use ecad_model::naming::numbering::Places;
use ecad_model::{Document, NetKey, PageId};

use crate::checks::*;
use crate::circuits::kept::Kept;
use crate::circuits::view::RatingView;
use ecad_model::look::Look;

/// The kept Checks list (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct Checks {
    by_key: BTreeMap<Key, Vec<Check>>,
    grid: Grid,
    /// Nets with conductors and no number (for the one Unnumbered row).
    unnumbered: BTreeSet<NetKey>,
    /// Provisional tags are on: the Unnumbered row is a warning, `… provisional`.
    provisional: bool,
    /// Bumped on every change, for views that copy the rows.
    pub revision: u64,
    /// Rows by severity (errors, warnings, info), kept by [`Checks::set`] (the badge reads
    /// them; nothing recounts the rows).
    counts: [usize; 3],
    /// Bumped only when some row actually changed (an update that re-checked subjects and found
    /// the same rows leaves it), for views that show the rows themselves (the Release dialog).
    pub changed: u64,
    /// The pages whose type has checks and references off.
    off: BTreeSet<PageId>,
}

impl Checks {
    /// The whole list, computed from scratch (open, reload, first show), its rating rows from the
    /// kept rating results `rating`.
    pub fn build_with(doc: &Document, tol: f64, m: &dyn Look, registry: &ecad_model::registry::data::Registry, rating: &Kept) -> Self {
        let mut ch = Checks { off: ecad_model::document::sheet::checks_off_pages(doc), ..Checks::default() };
        for r in doc.routes().keys() {
            ch.grid.insert(doc, *r);
        }
        let conn = doc.connectivity();
        for (id, net) in conn.nets() {
            ch.set(Key::Net(id), net_checks(doc, id, net));
            if !net.wires.is_empty() && net.numbers.is_empty() {
                ch.unnumbered.insert(id);
            }
        }
        for c in doc.wires().keys() {
            let v = conductor_checks(doc, *c);
            ch.set(Key::Wire(*c), v);
        }
        for r in doc.routes().keys() {
            let v = route_checks(doc, *r, &ch.grid, tol);
            ch.set(Key::Route(*r), v);
        }
        for t in doc.connection_points().keys() {
            ch.set(Key::ConnectionPoint(*t), connection_point_checks(doc, *t));
        }
        // One drawing order for the pass: the book is built at most once.
        let mut places = Places::new(doc);
        for c in doc.devices().keys() {
            ch.set(Key::Device(*c), component_checks_in(&mut places, *c, m));
        }
        for a in doc.aids().keys() {
            ch.set(Key::Aid(*a), aid_checks(doc, *a));
        }
        for k in doc.cables().keys() {
            ch.set(Key::CableReferences(*k), reference_checks(doc, *k));
            ch.set(Key::Cable(*k), cable_checks_in(&mut places, *k));
        }
        for page in doc.pages().keys() {
            ch.set(Key::Page(*page), page_checks(doc, *page));
        }
        for s in PROJECT_SOURCES {
            ch.set(s.key(), s.checks(doc, registry, m));
        }
        for r in rating.results.values() {
            ch.set(Key::Rating(r.subject), crate::circuits::rows::rows(doc, r));
        }
        ch.set(Key::RatingSummary, crate::circuits::rows::summary_rows(doc, rating));
        ch.provisional = ecad_model::naming::numbering::provisional_on(doc);
        ch.set_unnumbered();
        ch
    }

    /// Work out the cover's rows again (after a change the cover's map names).
    pub fn set_cover(&mut self, doc: &Document, m: &dyn Look) {
        self.set(Key::Cover, crate::pages::front::check_rows(doc, m));
        self.revision += 1;
    }

    fn set(&mut self, key: Key, mut v: Vec<Check>) {
        // No electrical row on a page with checks and references off.
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

    /// The rows sorted in book order.
    pub fn sorted(&self, doc: &Document) -> Vec<Check> {
        sort(doc, self.rows().cloned().collect())
    }

    /// Re-check the subjects in `scope` on `doc`, the state after the commit. `scope` holds what
    /// each of the commit's effects names on the state before and on the state after
    /// ([`Scope::note`]; `crate::views::ChecksView` notes them). The rating rows follow `rating`,
    /// already updated for the same commit: the subjects it re-checked ([`Kept::last`]).
    pub fn update(
        &mut self, doc: &Document, mut scope: Scope, tol: f64, m: &dyn Look, registry: &ecad_model::registry::data::Registry, rating: &RatingView,
    ) {
        let kept = rating.kept();
        if scope.all {
            *self = Checks { revision: self.revision + 1, changed: self.changed + 1, ..Checks::build_with(doc, tol, m, registry, kept) };
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
        let exempt = |n: &str| doc.settings().wiring.potentials.iter().any(|p| !p.numbered && p.prefix == n);
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
        for t in &scope.connection_points {
            self.set(Key::ConnectionPoint(*t), connection_point_checks(doc, *t));
        }
        // A motor's supply follows the potentials of its nets: the motors on the nets checked again.
        let mut devices = scope.devices.clone();
        for id in &nets {
            for t in conn.net(*id).into_iter().flat_map(|n| n.connection_points.iter()) {
                if let Some(c) = doc.connection_points().get(t).and_then(|x| x.device())
                    && crate::devices::supply::motor_of(doc, c).is_some()
                {
                    devices.insert(c);
                }
            }
        }
        let mut places = Places::new(doc);
        for c in &devices {
            self.set(Key::Device(*c), component_checks_in(&mut places, *c, m));
        }
        for a in &scope.aids {
            self.set(Key::Aid(*a), aid_checks(doc, *a));
        }
        for k in &scope.cable_references {
            self.set(Key::CableReferences(*k), reference_checks(doc, *k));
        }
        for k in &scope.cables {
            self.set(Key::Cable(*k), cable_checks_in(&mut places, *k));
        }
        // The dash budget of the pages the re-checked wires are on, and of the pages
        // that had a row (a wire removed or recoloured there).
        let mut pages: BTreeSet<PageId> = self.by_key.keys().filter_map(|k| if let Key::Page(p) = k { Some(*p) } else { None }).collect();
        for c in &scope.wires {
            pages.extend(crate::wires::routes::wire_pages(doc, *c));
        }
        pages.extend(routes.iter().filter_map(|r| doc.routes().get(r)).map(|r| r.page));
        for page in pages {
            let rows = if doc.pages().contains_key(&page) { page_checks(doc, page) } else { Vec::new() };
            self.set(Key::Page(page), rows);
        }
        // The rated subjects the ops reached, and the count rows.
        if !kept.last.is_empty() || rating.reached() {
            for s in &kept.last {
                let rows = kept.results.get(s).map(|r| crate::circuits::rows::rows(doc, r)).unwrap_or_default();
                self.set(Key::Rating(*s), rows);
            }
            self.set(Key::RatingSummary, crate::circuits::rows::summary_rows(doc, kept));
        }
        for s in PROJECT_SOURCES {
            if s.touched(&scope) {
                self.set(s.key(), s.checks(doc, registry, m));
            }
        }
        self.provisional = ecad_model::naming::numbering::provisional_on(doc);
        self.set_unnumbered();
        self.revision += 1;
    }
}
