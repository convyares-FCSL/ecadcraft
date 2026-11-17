//! The kept report models (see [`super`]'s update map). The kept PLC model's rules measure text
//! (an address wider than its cell), so building and updating take the sheet font metrics,
//! [`TextMetrics`]; ecad-cadcraft's `crate::outputs::Outputs` wraps this with today's
//! signatures, passing CADCraft's fonts.

use ecad_model::keys::PropsExt;
use std::collections::{BTreeMap, BTreeSet};

use ecad_model::book::LOCATION_KEY;
use ecad_model::{CableId, ComponentId, ConductorId, Document, End, ObjectRef, Op, PageId, Pt, Subject, TerminalId};

use super::*;
use crate::text_style::fonts::TextMetrics;

/// The kept report models (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct Outputs {
    devices: BTreeMap<ComponentId, DeviceRow>,
    /// Brief 047: items whose location is the project's, or none: a project location set reaches
    /// them. Devices never are (brief 052: a device's location is stored).
    from_project: BTreeSet<ComponentId>,
    from_none: BTreeSet<ComponentId>,
    /// Brief 047 (design 047 §4, §10): the BOM entries (main parts, part lines, items) and the
    /// rows they make, kept by key.
    entries: BTreeMap<(ComponentId, u8), Entry>,
    by_location: BTreeMap<(LocKey, PartKey), BTreeSet<EntrySort>>,
    by_part: BTreeMap<PartKey, BTreeSet<EntrySort>>,
    /// The nested layout's rows: main parts and items only (a device with part lines a row of its
    /// own), and the part lines per location, so its sheet count is O(locations) too.
    nested: BTreeMap<(LocKey, PartKey), BTreeSet<EntrySort>>,
    loc_nested_rows: BTreeMap<LocKey, usize>,
    loc_lines: BTreeMap<LocKey, usize>,
    /// Part key → its article numbers with how many entries carry each (§4.3).
    articles: BTreeMap<PartKey, BTreeMap<String, usize>>,
    /// Part keys with two or more article numbers.
    article_conflicts: BTreeSet<PartKey>,
    /// Lines whose `for` names sub-items the device lacks.
    for_missing: BTreeSet<(ComponentId, u8)>,
    /// `part.` keys ECAD does not read, per component.
    bad_part_keys: BTreeMap<ComponentId, Vec<String>>,
    /// Uncovered fuses per device or strip, and lines that are fuse inserts (any `for`, or a line
    /// on a fuse terminal device), per component.
    fuse_gaps: BTreeMap<ComponentId, usize>,
    inserts: BTreeMap<ComponentId, usize>,
    lines_no_part: usize,
    items_no_part: usize,
    /// The report settings the tables use (`bom.column.article`, `bom.lines`).
    pub view: BomView,
    wires: BTreeMap<ConductorId, WireRow>,
    /// Conductors by their net's potential (lower case; `""`: none), for wiring-table changes.
    by_potential: BTreeMap<String, BTreeSet<ConductorId>>,
    cables: BTreeMap<CableId, CableRow>,
    /// Rows of the BOM-by-location table per location (a heading each), for its sheet count.
    loc_rows: BTreeMap<LocKey, usize>,
    /// The styles the tables write codes and places in.
    style: ecad_model::ReferenceStyle,
    colour_style: ecad_model::wiring::ColourStyle,
    /// Location box checks by page.
    box_checks: BTreeMap<PageId, Vec<Check>>,
    no_part_number: usize,
    no_location: usize,
    /// Brief 052: devices with no function (one count row when some device has one).
    no_function: usize,
    /// Brief 028: the terminal strips (design 028 §11.1).
    pub strips: crate::strips::Strips,
    /// Brief 045: the PLC racks and cards (design 045 §11.1).
    pub plc: crate::plc::kept::Plc,
    /// Brief 046: the graphic strip pages' kept layout and indexes (design 046 §9).
    pub strip_pages: crate::strips::pages::StripPages,
    /// Brief 028: whether the last update changed a cable row (what a shown cable schedule page
    /// depends on; tester 028, defect 2).
    pub last_cables_changed: bool,
    /// The pages drawn from these models (BOM, terminal plan, cable schedule), kept from the page
    /// ops (review of PR #55, finding 1: no page scan per commit).
    generated: BTreeSet<PageId>,
    /// Which build these models are (unique in the run): a copy keyed on kept revisions also
    /// keys on this, so a rebuild (reload, branch switch) is never mistaken for the old models.
    pub build_id: u64,
    /// How many sorted row lists were made (tests: an unrelated edit makes none).
    pub sorts: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// The location box rows by severity (warnings, info): kept with the rows (brief 027's badge).
    box_counts: [usize; 2],
    /// Bumped only when the check rows may read differently (brief 027's Release dialog).
    pub checks_changed: u64,
    /// Brief 028's cable Checks rows, kept: `too-many-cores` warnings, `core-twice` warnings, and
    /// cables with no core count (one info row when any).
    cable_checks: [usize; 3],
    /// Brief 044 (design 044 §8): each connector's BOM rows, one per drawn half.
    connectors: BTreeMap<ComponentId, Vec<BomRow>>,
    /// Brief 044 (review of #81, 2): the half rows' groups, kept beside `loc_rows` / `by_part`:
    /// (location, group) and group → how many halves; location → how many groups.
    half_loc: BTreeMap<(LocKey, HalfKey), usize>,
    half_part: BTreeMap<HalfKey, usize>,
    half_loc_rows: BTreeMap<LocKey, usize>,
    /// How many times the half rows were grouped (tests: an unrelated edit groups none).
    pub half_groupings: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Brief 030 (review of PR #57, 4): devices that are not strips but carry `link.<n>` keys.
    links_off_strip: BTreeSet<ComponentId>,
    link_keys: BTreeMap<ComponentId, Vec<String>>,
    /// Brief 033 (review of #64, 2): the `continuation-clash` rows, kept. Worked out again only
    /// when a page is added, removed or renumbered, an issue changes the revision list, or a kept
    /// generated page's sheet count moves.
    continuation: Vec<Check>,
    /// How many times `Self::continuation` was worked out (tests: an unrelated edit makes none).
    pub continuation_builds: u64,
    /// Bumped on every change, for views that copy the rows.
    pub revision: u64,
    /// How many rows the last update recomputed (tests and bench: never the whole project).
    pub last_work: usize,
    /// Brief 046 (review): the last update's ops could move a page number or a sheet count that
    /// the document alone decides (a page's style or sheet): the frame's Total is worked out again.
    pub numbers_moved: bool,
}

impl Outputs {
    /// Every model, from scratch (open, reload, first show), with text measured in `m`.
    pub fn build_with(doc: &Document, m: &dyn TextMetrics) -> Outputs {
        let mut o = Outputs {
            style: doc.settings.reference_style(),
            colour_style: doc.settings.wiring.colour_style(),
            view: BomView::of(doc),
            ..Outputs::default()
        };
        let mut cache = BoxCache::default();
        for (_, c) in doc.tagged() {
            o.set_device(doc, c, &mut cache);
        }
        // Brief 047: items (tagless, on no page).
        for (c, comp) in &doc.components {
            if bom::is_item_component(comp) {
                o.set_device(doc, *c, &mut cache);
            }
        }
        let cx = PageCx::of(doc);
        for w in doc.conductors.keys() {
            o.set_wire(doc, *w, &cx);
        }
        for k in doc.cables.keys() {
            o.set_cable(doc, *k);
        }
        for p in &doc.page_order {
            o.set_box_checks(doc, *p);
        }
        o.strips = crate::strips::Strips::build(doc, &o, &cx);
        o.plc = crate::plc::kept::Plc::build_with(doc, m);
        o.strip_pages = crate::strips::pages::StripPages::build(doc, &o.strips);
        o.generated = doc.page_order.iter().copied().filter(|p| needs_outputs(doc, *p)).collect();
        static BUILDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        o.build_id = BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        o.refresh_continuation(doc);
        o
    }

    /// Work out the `continuation-clash` rows (design 033 §5.3).
    fn refresh_continuation(&mut self, doc: &Document) {
        let rows = crate::generated::continuation_checks(doc, Some(&*self));
        self.continuation_builds += 1;
        if rows != self.continuation {
            self.continuation = rows;
            self.checks_changed += 1;
        }
    }

    /// The sheet count of each kept generated page (BOM, terminal plan, cable schedule).
    fn generated_sheets(&self, doc: &Document) -> Vec<(PageId, usize)> {
        self.generated.iter().map(|p| (*p, crate::generated::sheets_of(doc, *p, Some(self)))).collect()
    }

    /// Whether `ops` can move a page number or a contents or revision list sheet count: pages
    /// added, removed or renumbered, a page's kind, or an issue.
    fn moves_page_numbers(ops: &[Op]) -> bool {
        ops.iter().any(|op| match op {
            Op::AddPage { .. } | Op::RemovePage { .. } => true,
            // Brief 046 (review): a page's style or sheet changes its sheet count on the document
            // alone, so the count before cannot be compared.
            Op::SetProperty { object: ObjectRef::Page(_), key, .. } => {
                key == ecad_model::book::NUMBER_KEY
                    || key == crate::generated::KEY
                    || key == crate::defaults::strip_pages::STYLE.key
                    || key == ecad_model::sheet::SHEET_KEY
            }
            Op::SetProperty { object: ObjectRef::Project, key, .. } => {
                (key.starts_with(crate::revisions::PREFIX) && key.ends_with(crate::revisions::ISSUED))
                    || key == crate::defaults::strip_pages::STYLE.key
            }
            Op::SetProperty { object: ObjectRef::Standard, key, .. } => key == crate::defaults::strip_pages::STYLE.key,
            Op::SetProjectSettings { before, after } => before.sheet != after.sheet,
            _ => false,
        })
    }

    /// The pages drawn from these models.
    pub fn generated_pages(&self) -> &BTreeSet<PageId> {
        &self.generated
    }

    /// How many BOM rows `group` lists, from kept counts (no rows made).
    pub fn bom_count(&self, group: Group) -> usize {
        let strips: usize = self.strips.bom_rows.values().sum();
        // Brief 044: grouped as `bom` groups them (O(connectors)).
        let halves = match group {
            Group::Location => self.half_loc.len(),
            Group::Part => self.half_part.len(),
        };
        match group {
            Group::Location if self.view.nested => self.nested.len() + self.loc_lines.values().sum::<usize>() + strips + halves,
            Group::Location => self.by_location.len() + strips + halves,
            Group::Part => self.by_part.len() + strips + halves,
        }
    }

    /// How many wire rows.
    pub fn wire_count(&self) -> usize {
        self.wires.len()
    }

    /// One cable's row.
    pub fn cable_row(&self, k: CableId) -> Option<&CableRow> {
        self.cables.get(&k)
    }

    /// How many cables (no sort).
    pub fn cable_count(&self) -> usize {
        self.cables.len()
    }

    /// The strips sorted by tag (counted in [`Outputs::sorts`]).
    pub fn sorted_strips(&self) -> Vec<&crate::strips::StripData> {
        self.sorts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.strips.sorted()
    }

    /// What `ops`, about to be committed on `before`, could change. Pair with `Outputs::update`.
    pub fn scope(&self, before: &Document, ops: &[Op]) -> Scope {
        let mut s = Scope::default();
        s.collect(before, ops);
        s
    }

    /// Recompute the rows in `scope` on `doc` (the state after), adding what `ops` (the whole
    /// committed transaction) name in this state.
    pub fn update_with(&mut self, doc: &Document, ops: &[Op], scope: Scope, m: &dyn TextMetrics) {
        let numbers = Self::moves_page_numbers(ops);
        self.numbers_moved = numbers;
        let before = self.generated_sheets(doc);
        self.update_rows(doc, ops, scope, m);
        // Brief 046: right after the strips (design 046 §9), also when no row was in scope.
        self.strip_pages.update(doc, ops, &self.strips);
        if numbers || self.generated_sheets(doc) != before {
            self.refresh_continuation(doc);
            self.revision += 1;
        }
    }

    fn update_rows(&mut self, doc: &Document, ops: &[Op], mut scope: Scope, m: &dyn TextMetrics) {
        // What this update changes, for the generated pages that depend on it (brief 028).
        self.last_cables_changed = false;
        self.strips.last_changed.clear();
        // Brief 045: the PLC model follows its own update map (design 045 §11.1).
        if self.plc.update_with(doc, ops, m) {
            self.checks_changed += 1;
            self.revision += 1;
        }
        for op in ops {
            let p = match op {
                Op::AddPage { id, .. } | Op::RemovePage { id, .. } => *id,
                Op::SetProperty { object: ObjectRef::Page(p), key, .. } if key == crate::generated::KEY => *p,
                _ => continue,
            };
            if needs_outputs(doc, p) {
                self.generated.insert(p);
            } else {
                self.generated.remove(&p);
            }
        }
        scope.collect(doc, ops);
        if scope.is_empty() {
            return;
        }
        let mut work = 0;
        self.style = doc.settings.reference_style();
        self.colour_style = doc.settings.wiring.colour_style();
        if scope.bom_view {
            self.view = BomView::of(doc);
        }
        for k in &scope.potentials {
            scope.conductors.extend(self.by_potential.get(k).into_iter().flatten().copied());
        }
        if scope.all_wires {
            scope.conductors.extend(self.wires.keys().copied());
        }
        if scope.sheet {
            let own = |p: &PageId| doc.pages.get(p).is_some_and(|x| x.properties.contains_key(ecad_model::sheet::SHEET_KEY));
            scope.route_pages.extend(doc.page_order.iter().filter(|p| !own(p)).copied());
        }
        if scope.project_location {
            scope.devices.extend(self.from_project.iter().copied());
            scope.devices.extend(self.from_none.iter().copied());
        }
        // Brief 036 (tester 036 j, o): a level's label, or a separator or level names a strip
        // reads, change the wire rows ending on that strip's terminals (only the position named by
        // a `term_label.<p>.<n>` key).
        for op in ops {
            let Op::SetProperty { object: ObjectRef::Component(c), key, .. } = op else { continue };
            let position = key.strip_prefix(crate::terminal_types::LABEL_PREFIX).and_then(|r| r.rsplit_once('.')).map(|(p, _)| p);
            if (position.is_none() && !crate::defaults::terminals::is_label_key(key)) || !crate::strips::is_strip(doc, *c) {
                continue;
            }
            for (t, x) in doc.terminals_of(*c) {
                if position.is_none_or(|p| x.label.as_deref() == Some(p)) {
                    scope.conductors.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                }
            }
        }
        // Brief 044 (design 044 §11.1): a connector naming or description default reaches the
        // connectors inheriting it (their half rows); a naming key also their pins' wire rows.
        for k in &scope.connector_keys {
            let naming = crate::defaults::connectors::is_naming_key(k);
            let own = |c: &ComponentId| naming && doc.components.get(c).is_some_and(|x| x.properties.contains_key(k.as_str()));
            let inheriting: Vec<ComponentId> = self.connectors.keys().filter(|c| !own(c)).copied().collect();
            for c in inheriting {
                scope.devices.insert(c);
                if crate::defaults::connectors::is_naming_key(k) {
                    for (t, _) in doc.terminals_of(c) {
                        scope.conductors.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                    }
                }
            }
        }
        for op in ops {
            let Op::SetProperty { object: ObjectRef::Component(c), key, .. } = op else { continue };
            if crate::defaults::connectors::is_naming_key(key) && crate::connectors::is_connector(doc, *c) {
                for (t, _) in doc.terminals_of(*c) {
                    scope.conductors.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                }
            }
        }
        for k in scope.strip_keys.iter().filter(|k| crate::defaults::terminals::is_label_key(k)) {
            for c in self.strips.inheriting(k) {
                for (t, _) in doc.terminals_of(c) {
                    scope.conductors.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                }
            }
        }
        let mut cache = BoxCache::default();
        for c in &scope.devices {
            let before = self.devices.get(c).map(|d| (d.tag.clone(), d.location.value.clone()));
            self.set_device(doc, *c, &mut cache);
            work += 1;
            let after = self.devices.get(c).map(|d| (d.tag.clone(), d.location.value.clone()));
            if before != after {
                for (t, _) in doc.terminals_of(*c) {
                    scope.conductors.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                }
            }
        }
        for p in &scope.route_pages {
            if doc.pages.contains_key(p) {
                for (_, r) in doc.representations_on(*p) {
                    if let Subject::Conductor(w) = r.subject {
                        scope.conductors.insert(w);
                    }
                }
            }
        }
        // Page positions and numbers only when a wire is in scope (review of PR #51, finding 3).
        let mut cx = if scope.conductors.is_empty() { None } else { Some(PageCx::of(doc)) };
        for (w, cx) in scope.conductors.iter().zip(std::iter::repeat_with(|| cx.as_ref()).flatten()) {
            self.set_wire(doc, *w, cx);
            work += 1;
            scope.cables.extend(doc.cables_of(*w));
        }
        for k in &scope.cables {
            self.set_cable(doc, *k);
            work += 1;
        }
        for p in &scope.pages {
            self.set_box_checks(doc, *p);
        }
        // Brief 028: the strips these rows touch (design 028 §11.1).
        let mut pages = scope.route_pages.clone();
        pages.extend(ecad_model::book::positions_changed(doc, ops));
        let mut strips = std::mem::take(&mut self.strips);
        let which = strips.touched(
            doc,
            &crate::strips::StripScope {
                devices: &scope.devices,
                conductors: &scope.conductors,
                cables: &scope.cables,
                pages: &pages,
                project_keys: &scope.strip_keys,
            },
        );
        if strips.note_devices(doc, &scope.devices) {
            self.checks_changed += 1;
        }
        let acc_only = strips.touched_acc(&scope.strip_keys, &which);
        if which.is_empty() {
            strips.last_work = 0;
            strips.last_changed.clear();
        } else {
            // The page positions worked out for the wires, if they were (O(pages) once). Counted
            // in the strips' own `last_work`, not in the rows'.
            let cx = cx.get_or_insert_with(|| PageCx::of(doc));
            strips.recompute(doc, &which, self, cx);
        }
        // Brief 030: a project default reaches only its inheriting strips' accessories.
        if !acc_only.is_empty() {
            strips.recompute_acc(doc, &acc_only);
        }
        // Brief 028's strip Checks rows read from the strips just recomputed (brief 027's key).
        if !strips.last_changed.is_empty() {
            self.checks_changed += 1;
        }
        self.strips = strips;
        self.last_work = work;
        self.revision += 1;
    }

    /// Verify mode (brief 028, design 028 §11.4): the kept strips against a rebuild on `doc`; a
    /// difference is put right and described (a bug, for the event log).
    pub fn verify_strips_with(&mut self, doc: &Document, m: &dyn TextMetrics) -> Option<String> {
        // Brief 045 (design 045 §11.2): the kept PLC model too.
        let plc = crate::plc::kept::Plc::build_with(doc, m);
        let plc_line = (!self.plc.same_as(&plc)).then(|| {
            let (work, n) = (self.plc.last_work, self.plc.recomputed);
            self.plc = plc;
            self.plc.last_work = work;
            self.plc.recomputed = n;
            "the PLC model was out of step; rebuilt".to_string()
        });
        let built = crate::strips::Strips::build(doc, self, &PageCx::of(doc));
        // Brief 046: the graphic pages' kept data against a rebuild from the kept strips.
        let pages = crate::strips::pages::StripPages::build(doc, &self.strips);
        let pages_ok = self.strip_pages.same_as(&pages);
        if !pages_ok {
            self.strip_pages.adopt(pages);
        }
        if self.strips.same_as(&built) {
            let pages_line = (!pages_ok).then(|| "the terminal strip page layout was out of step; rebuilt".to_string());
            return match (plc_line, pages_line) {
                (Some(a), Some(b)) => Some(format!("{a}; {b}")),
                (a, b) => a.or(b),
            };
        }
        let work = self.strips.last_work;
        self.strips = built;
        self.strips.last_work = work;
        // The page layout follows the rebuilt strips (review of 046, finding 2).
        self.strip_pages.adopt(crate::strips::pages::StripPages::build(doc, &self.strips));
        let line = "the terminal strip model was out of step; rebuilt".to_string();
        Some(plc_line.map_or_else(|| line.clone(), |l| format!("{line}; {l}")))
    }

    fn remove_device(&mut self, c: ComponentId) {
        self.remove_connector(c);
        let Some(old) = self.devices.remove(&c) else { return };
        self.from_project.remove(&c);
        self.from_none.remove(&c);
        self.no_part_number -= usize::from(old.part_number.is_none());
        self.no_location -= usize::from(old.location.value.is_none());
        self.no_function -= usize::from(old.function.key.is_none());
    }

    /// Brief 047: take every entry of `c` out of the rows and counts.
    fn remove_entries(&mut self, c: ComponentId) {
        // An item's location source (a device's is taken out by `remove_device`).
        self.from_project.remove(&c);
        self.from_none.remove(&c);
        let keys: Vec<(ComponentId, u8)> = self.entries.range((c, 0)..=(c, u8::MAX)).map(|(k, _)| *k).collect();
        for k in keys {
            let Some(e) = self.entries.remove(&k) else { continue };
            let (sort, pk, lk) = (e.sort(), e.part_key(), e.loc_key());
            if remove_from(&mut self.by_location, &(lk.clone(), pk.clone()), &sort) {
                count_down(&mut self.loc_rows, &lk);
            }
            remove_from(&mut self.by_part, &pk, &sort);
            if e.line == 0 {
                let nk = (lk.clone(), self.nested_key(&e, &pk));
                if remove_from(&mut self.nested, &nk, &sort) {
                    count_down(&mut self.loc_nested_rows, &lk);
                }
            } else {
                count_down(&mut self.loc_lines, &lk);
                self.lines_no_part -= usize::from(e.part_number.is_none());
            }
            if e.item {
                self.items_no_part -= usize::from(e.part_number.is_none());
            }
            if e.item || e.line > 0 {
                self.checks_changed += 1;
            }
            if let Some(a) = &e.article_number
                && e.part_number.is_some()
            {
                let set = self.articles.entry(pk.clone()).or_default();
                if let Some(n) = set.get_mut(a) {
                    *n -= 1;
                    if *n == 0 {
                        set.remove(a);
                    }
                }
                if set.is_empty() {
                    self.articles.remove(&pk);
                }
                if self.articles.get(&pk).is_none_or(|s| s.len() < 2) && self.article_conflicts.remove(&pk) {
                    self.checks_changed += 1;
                }
            }
            if self.for_missing.remove(&k) {
                self.checks_changed += 1;
            }
        }
        if self.bad_part_keys.remove(&c).is_some() {
            self.checks_changed += 1;
        }
        if self.fuse_gaps.remove(&c).is_some() | self.inserts.remove(&c).is_some() {
            self.checks_changed += 1;
        }
    }

    /// The nested layout's row key of main part or item `e`: a device with part lines is a row of
    /// its own (its lines follow it).
    fn nested_key(&self, e: &Entry, pk: &PartKey) -> PartKey {
        let has_lines = self.entries.range((e.component, 1)..=(e.component, u8::MAX)).next().is_some();
        if has_lines { pk.alone((e.component, 0)) } else { pk.clone() }
    }

    /// Brief 047: put entry `e` into the rows and counts.
    fn add_entry(&mut self, e: Entry) {
        let (sort, pk, lk) = (e.sort(), e.part_key(), e.loc_key());
        let group = self.by_location.entry((lk.clone(), pk.clone())).or_default();
        if group.is_empty() {
            *self.loc_rows.entry(lk.clone()).or_default() += 1;
        }
        group.insert(sort.clone());
        self.by_part.entry(pk.clone()).or_default().insert(sort.clone());
        if e.line == 0 {
            let nk = (lk.clone(), self.nested_key(&e, &pk));
            let group = self.nested.entry(nk).or_default();
            if group.is_empty() {
                *self.loc_nested_rows.entry(lk).or_default() += 1;
            }
            group.insert(sort);
        } else {
            *self.loc_lines.entry(lk).or_default() += 1;
            self.lines_no_part += usize::from(e.part_number.is_none());
        }
        if e.item {
            self.items_no_part += usize::from(e.part_number.is_none());
        }
        if e.item || e.line > 0 {
            self.checks_changed += 1;
        }
        if let Some(a) = &e.article_number
            && e.part_number.is_some()
        {
            let set = self.articles.entry(pk.clone()).or_default();
            *set.entry(a.clone()).or_default() += 1;
            if set.len() > 1 && self.article_conflicts.insert(pk) {
                self.checks_changed += 1;
            }
        }
        if !e.missing.is_empty() {
            self.for_missing.insert((e.component, e.line));
            self.checks_changed += 1;
        }
        self.entries.insert((e.component, e.line), e);
    }

    /// Brief 047: the entries of `c` (its main part and part lines, or the item it is), and its
    /// fuse coverage.
    fn set_entries(&mut self, doc: &Document, c: ComponentId, main: Option<Entry>) {
        let Some(comp) = doc.components.get(&c) else { return };
        let item = main.as_ref().is_some_and(|e| e.item);
        // Review of #83, 1: an item's `qty` or `unit` that does not parse is listed, not silently 1.
        let (lines, bad) = if item { (Vec::new(), bom::item_bad_keys(&comp.properties)) } else { bom::lines(&comp.properties) };
        if !bad.is_empty() {
            self.bad_part_keys.insert(c, bad);
            self.checks_changed += 1;
        }
        let (tag, location) = match self.devices.get(&c) {
            Some(d) => (d.tag.clone(), d.location.value.clone()),
            None => (String::new(), None),
        };
        // Brief 044 (design 044 §8.3): a connector's lines go under a half (`part.<n>.half`, else
        // its first drawn half), with that half's tag and location.
        let halves = if item { Vec::new() } else { crate::connectors::half_rows(doc, c, &mut BoxCache::default()) };
        // Sub-items only where `for` can name some or fuses can be uncovered.
        let fuseterm = !item && bom::is_fuse_terminal(doc, c);
        let subs = if item || fuseterm { bom::Subs::None } else { bom::sub_items(doc, c) };
        let mut covered: BTreeSet<&str> = BTreeSet::new();
        let mut inserts = 0;
        let mut entries = Vec::new();
        for l in &lines {
            let (have, missing): (Vec<String>, Vec<String>) = l.subs.iter().cloned().partition(|s| subs_has(&subs, s));
            covered.extend(l.subs.iter().map(String::as_str));
            inserts += usize::from(l.for_text.is_some() || fuseterm);
            let half = halves.iter().find(|h| Some(h.side) == l.half).or(halves.first());
            entries.push(Entry {
                component: c,
                line: l.n,
                item: false,
                tag: half.map_or_else(|| tag.clone(), |h| h.tag.clone()),
                location: half.map_or_else(|| location.clone(), |h| h.location.value.clone()),
                description: l.description.clone(),
                maker: l.maker.clone(),
                part_number: l.part_number.clone(),
                article_number: l.article_number.clone(),
                qty: l.qty,
                metres: l.metres,
                kit: l.kit.clone(),
                subs: have,
                missing,
            });
        }
        let gaps = if fuseterm {
            usize::from(!lines.iter().any(|l| l.for_text.is_none()))
        } else {
            subs.fuses().iter().filter(|f| !covered.contains(f.as_str())).count()
        };
        // Lines first, so a main part knows (nested layout) that its device has some.
        for e in entries {
            self.add_entry(e);
        }
        if let Some(m) = main {
            self.add_entry(m);
        }
        if gaps > 0 {
            self.fuse_gaps.insert(c, gaps);
            self.checks_changed += 1;
        }
        if inserts > 0 {
            self.inserts.insert(c, inserts);
            self.checks_changed += 1;
        }
    }

    /// Brief 047: an item's entry and location (its own, else the project's; design 047 §3).
    fn set_item(&mut self, doc: &Document, c: ComponentId) {
        let Some(comp) = doc.components.get(&c) else { return };
        let props = &comp.properties;
        // Brief 052: stored values are entry keys of the project's list; shown by their codes.
        let shown = |k: Option<String>| k.map(|k| (crate::designations::code_of(doc, crate::designations::Aspect::Location, &k), k));
        let location = match shown(clean(props.get(LOCATION_KEY))) {
            Some((value, key)) => Located { value, source: Source::Device, key: Some(key) },
            None => match shown(clean(doc.properties.get(LOCATION_KEY))) {
                Some((value, key)) => Located { value, source: Source::Project, key: Some(key) },
                None => Located { value: None, source: Source::None, key: None },
            },
        };
        match location.source {
            Source::Project => {
                self.from_project.insert(c);
            }
            Source::None => {
                self.from_none.insert(c);
            }
            _ => {}
        }
        let metres = props.get(bom::UNIT_KEY).is_some_and(|u| bom::parse_unit(u).unwrap_or(false));
        let qty = props.get(bom::QTY_KEY).and_then(|q| bom::parse_qty(q, metres).ok()).unwrap_or(Qty::ONE);
        let e = Entry {
            component: c,
            line: 0,
            item: true,
            tag: String::new(),
            location: location.value,
            description: clean(props.get("description")),
            maker: clean(props.get("maker")),
            part_number: clean(props.get("part_number")),
            article_number: clean(props.get(bom::ARTICLE_KEY)),
            qty,
            metres,
            kit: None,
            subs: Vec::new(),
            missing: Vec::new(),
        };
        self.set_entries(doc, c, Some(e));
    }

    fn set_device(&mut self, doc: &Document, c: ComponentId, cache: &mut BoxCache) {
        self.remove_device(c);
        self.remove_entries(c);
        if bom::is_item(doc, c) {
            self.set_item(doc, c);
            return;
        }
        // Review of PR #57, 4: a link key on a device that is not a strip.
        let misplaced = !crate::strips::is_strip(doc, c)
            && doc.components.get(&c).is_some_and(|x| x.properties.keys().any(|k| ecad_model::links::link_number(k).is_some()));
        if misplaced {
            let keys = doc
                .components
                .get(&c)
                .map(|x| x.properties.keys().filter(|k| ecad_model::links::link_number(k).is_some()).cloned().collect())
                .unwrap_or_default();
            self.link_keys.insert(c, keys);
            self.links_off_strip.insert(c);
        } else if self.links_off_strip.remove(&c) {
            self.link_keys.remove(&c);
            self.checks_changed += 1;
        }
        if misplaced {
            self.checks_changed += 1;
        }
        if !locations::is_device(doc, c) {
            return;
        }
        let Some(comp) = doc.components.get(&c) else { return };
        let props = &comp.properties;
        let location = locations::resolve_with(doc, c, cache);
        let goto = crate::reps::goto(doc, c, None).and_then(|rid| doc.representations.get(&rid).map(|r| (rid, r.page, locations::anchor(doc, r))));
        let row = DeviceRow {
            component: c,
            tag: props.get(ecad_model::tag::TAG_KEY).cloned().unwrap_or_default(),
            description: clean(props.get("description")),
            maker: clean(props.get("maker")),
            part_number: clean(props.get("part_number")),
            article_number: clean(props.get(bom::ARTICLE_KEY)),
            location,
            function: locations::resolve_function(doc, c),
            goto,
        };
        // Brief 044 (design 044 §8.1): a connector has one BOM row per drawn half, kept apart;
        // its part lines are entries (brief 047), each under its half (`part.<n>.half`).
        let connector = crate::connectors::is_connector(doc, c);
        if connector {
            let rows = connector_rows(doc, c, cache);
            self.set_connector(c, rows);
        }
        // Brief 028 (Q13): a strip has one BOM row per terminal type, from the strip model; its
        // part lines are entries like a device's (brief 047).
        let main = (!crate::strips::is_strip(doc, c) && !connector).then(|| Entry {
            component: c,
            line: 0,
            item: false,
            tag: row.tag.clone(),
            location: row.location.value.clone(),
            description: row.description.clone(),
            maker: row.maker.clone(),
            part_number: row.part_number.clone(),
            article_number: row.article_number.clone(),
            qty: Qty::ONE,
            metres: false,
            kit: None,
            subs: Vec::new(),
            missing: Vec::new(),
        });
        self.no_part_number += usize::from(row.part_number.is_none());
        self.no_location += usize::from(row.location.value.is_none());
        self.no_function += usize::from(row.function.key.is_none());
        self.devices.insert(c, row);
        self.set_entries(doc, c, main);
    }

    fn end_cell(&self, doc: &Document, t: Option<TerminalId>, at: Option<(PageId, Pt)>, cx: &PageCx) -> EndCell {
        let order = at.map_or((usize::MAX, 0, 0), |(p, a)| {
            let pos = cx.pos(p);
            let place = cx.place(doc, p, a);
            (pos, place.as_ref().map_or(0, |x| x.column), place.as_ref().map_or(0, |x| x.row))
        });
        let Some(t) = t else { return EndCell { order, ..EndCell::default() } };
        let Some(term) = doc.terminals.get(&t) else { return EndCell { order, ..EndCell::default() } };
        let c = term.component;
        match crate::wires::kind_of(doc, c) {
            Some(ecad_model::connectivity::JUNCTION) => {
                EndCell { junction: Some(at.and_then(|(p, a)| cx.place(doc, p, a))), order, ..EndCell::default() }
            }
            Some(ecad_model::connectivity::CONTINUATION) => {
                let name = doc.components.get(&c).and_then(|x| x.properties.get(ecad_model::connectivity::NAME)).cloned().unwrap_or_default();
                EndCell { name: Some(format!("{name} >")), order, continuation: true, ..EndCell::default() }
            }
            _ => {
                let label = crate::terminal_types::terminal_name(doc, t);
                let device = self.devices.get(&c);
                let tag = device.map(|d| d.tag.clone()).or_else(|| crate::devices::tag_of(doc, c).map(str::to_string)).unwrap_or_else(|| "?".into());
                // Brief 044: a connector pin's end is named by its half (`-X10.S:3`).
                let name = crate::connectors::pin_name(doc, t).unwrap_or_else(|| format!("{tag}:{label}"));
                let location = match device {
                    Some(d) => d.location.value.clone(),
                    None => locations::resolve(doc, c).value,
                };
                EndCell { name: Some(name), location, device: Some(c), order, junction: None, continuation: false }
            }
        }
    }

    /// The cell of conductor end `t` drawn at `at` (brief 028: a terminal's far end).
    pub(crate) fn end_cell_of(&self, doc: &Document, t: Option<TerminalId>, at: Option<(PageId, Pt)>, cx: &PageCx) -> EndCell {
        self.end_cell(doc, t, at, cx)
    }

    fn set_wire(&mut self, doc: &Document, w: ConductorId, cx: &PageCx) {
        if let Some(old) = self.wires.remove(&w) {
            let k = old.potential.as_deref().unwrap_or("").trim().to_lowercase();
            if let Some(set) = self.by_potential.get_mut(&k) {
                set.remove(&w);
                if set.is_empty() {
                    self.by_potential.remove(&k);
                }
            }
        }
        let Some(cond) = doc.conductors.get(&w) else { return };
        // Routes in book order.
        let mut routes: Vec<(usize, Vec<Pt>, PageId)> = doc
            .representations_of(Subject::Conductor(w))
            .filter_map(|(_, r)| {
                let pts = r.items.iter().filter_map(|i| doc.items.get(i)).find_map(|i| match &i.kind {
                    ecad_model::ItemKind::Primitive(ecad_model::Primitive::Polyline { vertices, .. }) => Some(vertices.clone()),
                    ecad_model::ItemKind::Primitive(ecad_model::Primitive::Line { a, b }) => Some(vec![*a, *b]),
                    _ => None,
                })?;
                let pos = cx.pos(r.page);
                Some((pos, pts, r.page))
            })
            .collect();
        routes.sort_by_key(|(pos, _, _)| *pos);
        let a_at = routes.first().and_then(|(_, v, p)| v.first().map(|x| (*p, *x)));
        let b_at = routes.last().and_then(|(_, v, p)| v.last().map(|x| (*p, *x)));
        let mid = routes.first().and_then(|(_, v, p)| {
            let (Some(a), Some(b)) = (v.first(), v.get(1).or(v.first())) else { return None };
            Some((*p, Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)))
        });
        let mut ends = [self.end_cell(doc, cond.end(End::A), a_at, cx), self.end_cell(doc, cond.end(End::B), b_at, cx)];
        if ends[1].order < ends[0].order {
            ends.swap(0, 1);
        }
        let [from, to] = ends;
        let conn = doc.connectivity();
        let net = conn.net_of_conductor(w).and_then(|id| conn.net(id));
        let pot = net.map(|n| ecad_model::wiring::net_potential(doc, n));
        // Brief 050b: the wire's category gives what the wire has none of its own.
        let look = crate::styles::wire::wire_look(doc, w);
        let cross_section = look.cross_section.as_ref().map(|v| v.0.clone());
        let colour = look.colour.as_ref().map(|v| v.0.clone());
        let colour_own = look.colour.as_ref().is_some_and(|v| v.1 == crate::defaults::Source::Object);
        let cross_section_own = look.cross_section.as_ref().is_some_and(|v| v.1 == crate::defaults::Source::Object);
        let category = look.category.clone().map(|id| {
            let n = crate::styles::name(doc, "wire", &id);
            (id, n)
        });
        let part = look.category.as_ref().and_then(|id| {
            let f = |k: &str| crate::styles::field_of(doc, "wire", id, k, None).and_then(|r| crate::styles::wire::filled(&r));
            let pn = f("part_number")?;
            let allowance = crate::styles::field_of(doc, "wire", id, "allowance", None).map_or(0.0, |r| r.number());
            Some((pn, f("maker"), f("description"), allowance))
        });
        let cable = doc
            .cables_of(w)
            .into_iter()
            .filter_map(|k| {
                let cab = doc.cables.get(&k)?;
                let tag = cab.properties.text(ecad_model::keys::TAG).map(str::to_string).unwrap_or_else(|| "?".into());
                let i = cab.members.iter().position(|m| m.conductor == w)?;
                let core = cab.members.get(i).and_then(|m| m.core.clone()).unwrap_or_else(|| (i + 1).to_string());
                Some(format!("{tag} core {core}"))
            })
            .collect::<Vec<_>>()
            .join(", ");
        // Brief 028 (design 028 §9.2): a wire between two terminals of one strip is a bridge.
        let cable = match crate::strips::bridge_of(doc, w) {
            Some(strip) if cable.is_empty() => format!("Bridge {}", crate::devices::tag_of(doc, strip).unwrap_or("?")),
            _ => cable,
        };
        let page = mid.and_then(|(p, a)| cx.place(doc, p, a));
        let potential = pot.and_then(|p| p.name);
        self.by_potential.entry(potential.as_deref().unwrap_or("").trim().to_lowercase()).or_default().insert(w);
        self.wires.insert(
            w,
            WireRow {
                conductor: w,
                wire_number: net.and_then(|n| ecad_model::wiring::net_number(doc, n)),
                from,
                to,
                cross_section,
                colour,
                potential,
                category,
                colour_own,
                cross_section_own,
                part,
                cable,
                page,
                go: mid,
            },
        );
    }

    fn set_cable(&mut self, doc: &Document, k: CableId) {
        let before = self.cables.get(&k).cloned();
        self.set_cable_row(doc, k);
        if self.cables.get(&k) != before.as_ref() {
            self.last_cables_changed = true;
            let rows = |r: Option<&CableRow>| -> [usize; 3] {
                r.map_or([0, 0, 0], |r| {
                    let known = r.count.offer.is_some();
                    // Brief 059 (design 059 §6): a cable not drawn has no core-count row.
                    [usize::from(known && !r.count.over.is_empty()), r.count.twice.len(), usize::from(!known && r.drawn)]
                })
            };
            let (old, new) = (rows(before.as_ref()), rows(self.cables.get(&k)));
            for ((n, o), a) in self.cable_checks.iter_mut().zip(old).zip(new) {
                *n = n.saturating_sub(o) + a;
            }
            if old != new {
                self.checks_changed += 1;
            }
        }
    }

    fn set_cable_row(&mut self, doc: &Document, k: CableId) {
        let Some(cab) = doc.cables.get(&k) else {
            self.cables.remove(&k);
            return;
        };
        let p = |key: &str| clean(cab.properties.get(key));
        // Brief 032 §3.5: cores are counted by chain; each chain is shown by its first member,
        // with the first stored label along it.
        let (heads, tails) = chain_heads(doc, k, cab);
        let mut locs: BTreeSet<(LocKey, String)> = BTreeSet::new();
        let mut missing = false;
        // Brief 028: the devices at the cores' ends, per location.
        let mut at_loc: BTreeMap<String, BTreeSet<((String, u64, String), ComponentId)>> = BTreeMap::new();
        for m in &cab.members {
            let Some(cond) = doc.conductors.get(&m.conductor) else { continue };
            for t in cond.ends.iter().flatten() {
                let Some(c) = doc.terminals.get(t).map(|x| x.component) else { continue };
                if crate::wires::kind_of(doc, c).is_some() {
                    continue;
                }
                let (loc, tag) = match self.devices.get(&c) {
                    Some(d) => (d.location.value.clone(), d.tag.clone()),
                    None => (locations::resolve(doc, c).value, crate::devices::tag_of(doc, c).unwrap_or("?").to_string()),
                };
                match loc {
                    Some(l) => {
                        at_loc.entry(l.clone()).or_default().insert((tag_key(&tag), c));
                        locs.insert((loc_key(Some(&l)), l));
                    }
                    None => missing = true,
                }
            }
        }
        let tags_at = |l: &Option<String>| -> Vec<String> {
            let mut seen = BTreeSet::new();
            l.iter()
                .flat_map(|l| l.split(", "))
                .flat_map(|l| at_loc.get(l).into_iter().flatten())
                .filter(|(_, c)| seen.insert(*c))
                .map(|((_, _, t), _)| t.clone())
                .collect()
        };
        let list: Vec<String> = locs.into_iter().map(|(_, l)| l).collect();
        let (from, to, reason) = match (missing, list.as_slice()) {
            (true, _) => (None, None, Some("A device at one of its cores' ends has no location.".to_string())),
            (false, []) => (None, None, Some("No core ends at a device.".to_string())),
            (false, [one]) => (Some(one.clone()), Some(one.clone()), None),
            (false, [a, rest @ ..]) => (Some(a.clone()), Some(rest.join(", ")), None),
        };
        let (from_devices, to_devices) = (tags_at(&from), tags_at(&to));
        let go = doc.representations_of(Subject::Cable(k)).find_map(|(_, r)| {
            let at = r.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
                ecad_model::ItemKind::Primitive(ecad_model::Primitive::Line { a, .. }) => Some(*a),
                ecad_model::ItemKind::Primitive(ecad_model::Primitive::Polyline { vertices, .. }) => vertices.first().copied(),
                _ => None,
            })?;
            Some((r.page, at))
        });
        self.cables.insert(
            k,
            CableRow {
                cable: k,
                tag: p("tag"),
                kind: p("type"),
                cores: heads.len(),
                from_location: from,
                to_location: to,
                location_reason: reason,
                maker: p("maker"),
                length: p("length"),
                designation: p("designation"),
                go,
                count: crate::strips::count_cores(crate::styles::cable::offer(doc, k), &heads),
                members: heads.iter().map(|m| m.conductor).collect(),
                member_cores: heads.iter().map(|m| m.core.clone()).collect(),
                tails,
                from_devices,
                to_devices,
                drawn: crate::cables::is_drawn(doc, k),
                planned: crate::cables::Side::BOTH.map(|side| crate::cables::plan_words(doc, k, side)),
                glands: crate::cables::Side::BOTH.map(|side| crate::cables::gland(doc, k, side).map(|(g, _)| g)),
                shield: crate::cables::shield(doc, k).map(|(w, _)| w).filter(|w| *w != crate::cables::Shield::None).map(|w| w.word().to_string()),
            },
        );
    }

    fn set_box_checks(&mut self, doc: &Document, page: PageId) {
        if !doc.pages.contains_key(&page) {
            self.box_checks.remove(&page);
            return;
        }
        let boxes = locations::boxes_on(doc, page);
        let mut out = Vec::new();
        for b in &boxes {
            if locations::crosses_itself(&b.vertices) {
                out.push(Check {
                    kind: Kind::BoxCrosses,
                    page: Some(page),
                    at: b.vertices.first().copied(),
                    object: Object::Component(b.component),
                    which: b.location.clone().unwrap_or_else(|| "+?".into()),
                    fix: None,
                });
            }
        }
        for (i, a) in boxes.iter().enumerate() {
            for b in boxes.iter().skip(i + 1) {
                if let Some(at) = crossing(&a.vertices, &b.vertices) {
                    out.push(Check {
                        kind: Kind::BoxesOverlap,
                        page: Some(page),
                        at: Some(at),
                        object: Object::Component(a.component),
                        which: format!("{} and {}", a.location.as_deref().unwrap_or("+?"), b.location.as_deref().unwrap_or("+?")),
                        fix: None,
                    });
                }
            }
        }
        let old = self.box_checks.get(&page);
        if old.map_or(out.is_empty(), |o| *o == out) {
            return;
        }
        let tally = |v: &[Check]| {
            let w = v.iter().filter(|c| c.severity() == crate::checks::Severity::Warning).count();
            [w, v.len() - w]
        };
        let before = old.map_or([0, 0], |o| tally(o));
        let now = tally(&out);
        for i in 0..2 {
            if let (Some(n), Some(b), Some(a)) = (self.box_counts.get_mut(i), before.get(i), now.get(i)) {
                *n = n.saturating_sub(*b) + a;
            }
        }
        self.checks_changed += 1;
        if out.is_empty() {
            self.box_checks.remove(&page);
        } else {
            self.box_checks.insert(page, out);
        }
    }

    /// The location rows of the Checks list (design 024 §8.3).
    pub fn checks(&self) -> Vec<Check> {
        let mut out: Vec<Check> = self.box_checks.values().flatten().cloned().collect();
        out.extend(self.continuation.iter().cloned());
        let (n, l) = (self.no_part_number, self.lines_no_part);
        if n > 0 || l > 0 {
            let devices = if n == 1 { "1 device".to_string() } else { format!("{n} devices") };
            let lines = if l == 1 { "1 part line".to_string() } else { format!("{l} part lines") };
            let which = match (n, l) {
                (_, 0) => format!("{devices} {} no part number", if n == 1 { "has" } else { "have" }),
                (0, _) => format!("{lines} {} no part number", if l == 1 { "has" } else { "have" }),
                _ => format!("{devices} and {lines} have no part number"),
            };
            out.push(Check { kind: Kind::NoPartNumber, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        out.extend(self.bom_checks());
        let n = self.no_location;
        if n > 0 && n < self.devices.len() {
            let which = if n == 1 { "1 device has no location".to_string() } else { format!("{n} devices have no location") };
            out.push(Check { kind: Kind::NoLocation, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        // Brief 052 (design 052 §6): `function-none`, as `location-none`.
        let n = self.no_function;
        if n > 0 && n < self.devices.len() {
            let which = if n == 1 { "1 device has no function".to_string() } else { format!("{n} devices have no function") };
            out.push(Check { kind: Kind::NoFunction, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        out.extend(self.strip_checks());
        out.extend(self.plc.checks());
        out
    }

    /// The Checks rows of brief 028 (design 028 §8), from the kept strip and cable rows.
    pub fn strip_checks(&self) -> Vec<Check> {
        let mut out = Vec::new();
        for s in self.strips.strips.values() {
            // Brief 036 (§7.1): per type; with no type codes, 028's row for the whole strip.
            for (tc, offered) in s.over_offer() {
                let legacy = s.offer.is_some_and(|o| o.counts.iter().skip(2).all(|n| *n == 0)) && s.types.iter().all(|t| t.ty.legacy());
                let hi = if legacy { s.highest() } else { s.highest_of(tc.ty) };
                let which = if legacy {
                    format!("{}: {} terminals drawn, strip offers {offered}", s.tag, s.drawn)
                } else {
                    format!("{}: {} {} terminals drawn, strip offers {offered}", s.tag, tc.drawn, tc.ty.word)
                };
                out.push(Check {
                    kind: Kind::TooManyTerminals,
                    page: hi.and_then(|r| r.go).map(|(p, _)| p),
                    at: hi.and_then(|r| r.go).map(|(_, a)| a),
                    object: hi.and_then(|r| r.terminal).map_or(Object::Component(s.component), Object::Terminal),
                    which,
                    fix: Some(Fix::OfferDrawn(s.component)),
                });
            }
            for (label, t) in &s.twice {
                let row = s.row_of(*t);
                let go = row.and_then(|r| r.go);
                // Brief 036: a level of a multi-level position drawn twice goes to the next free one.
                let multi = row.is_some_and(|r| r.ty.multi() || r.ty.breaks);
                out.push(Check {
                    kind: if multi { Kind::LevelTwice } else { Kind::TerminalTwice },
                    page: go.map(|(p, _)| p),
                    at: go.map(|(_, a)| a),
                    object: Object::Terminal(*t),
                    which: format!("{}:{label} is drawn twice", s.tag),
                    fix: Some(if multi { Fix::MoveToNextFree(*t) } else { Fix::RenumberStrip(s.component) }),
                });
            }
            for (pos, t) in &s.mixed {
                let row = s.row_of(*t);
                let go = row.and_then(|r| r.go);
                let first = s.rows.iter().find(|r| r.label == *pos).map_or("?", |r| r.ty.word);
                let other = s.other_type_word(*t);
                out.push(Check {
                    kind: Kind::PositionMixed,
                    page: go.map(|(p, _)| p),
                    at: go.map(|(_, a)| a),
                    object: Object::Terminal(*t),
                    which: format!("{}:{pos} mixes {first} and {other} symbols", s.tag),
                    fix: Some(Fix::MoveToNewPosition(*t)),
                });
            }
            for (n, items) in &s.level_missing {
                let go = s.rows.iter().find(|r| r.go.is_some()).and_then(|r| r.go);
                let first = items.first().cloned().unwrap_or_default();
                let pos = ecad_model::links::split_level(&first).map(|(p, _)| p.to_string()).unwrap_or_default();
                let levels = s.rows.iter().filter(|r| r.label == pos).count();
                out.push(Check {
                    kind: Kind::LevelMissing,
                    page: go.map(|(p, _)| p),
                    at: go.map(|(_, a)| a),
                    object: Object::Component(s.component),
                    which: format!(
                        "{}: link {n} names {}, but {}:{pos} has {levels} level{}",
                        s.tag,
                        items.join(", "),
                        s.tag,
                        if levels == 1 { "" } else { "s" }
                    ),
                    fix: Some(Fix::LevelMissing(s.component, u16::try_from(*n).unwrap_or(u16::MAX))),
                });
            }
            out.extend(accessory_checks(s));
        }
        // Brief 036 (§8): fuse terminals that are devices of their own, and fuse positions with no
        // rating: one Info row each.
        let fuses = self.strips.fuse_devices.len();
        if fuses > 0 {
            let which = if fuses == 1 {
                "1 fuse terminal is a device of its own".to_string()
            } else {
                format!("{fuses} fuse terminals are devices of their own")
            };
            out.push(Check { kind: Kind::FuseDevice, page: None, at: None, object: Object::None, which, fix: Some(Fix::FuseDevices) });
        }
        let unrated: usize = self.strips.strips.values().map(|s| s.no_rating).sum();
        if unrated > 0 {
            let which = if unrated == 1 { "1 fuse position has no rating".to_string() } else { format!("{unrated} fuse positions have no rating") };
            out.push(Check { kind: Kind::NoFuseRating, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        for c in &self.links_off_strip {
            let tag = self.devices.get(c).map_or_else(|| "?".to_string(), |d| d.tag.clone());
            out.push(Check {
                kind: Kind::LinkOnNonStrip,
                page: None,
                at: None,
                object: Object::Component(*c),
                which: format!("{tag} has link keys but is not a terminal strip"),
                fix: Some(Fix::LinkOnNonStrip(*c)),
            });
        }
        if self.strips.acc_no_part > 0 {
            let n = self.strips.acc_no_part;
            let which =
                if n == 1 { "1 strip accessory has no part number".to_string() } else { format!("{n} strip accessories have no part number") };
            out.push(Check { kind: Kind::NoAccessoryPart, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        let mut unknown = 0;
        for r in self.cables.values() {
            let tag = r.tag.clone().unwrap_or_else(|| "?".into());
            let Some(offer) = &r.count.offer else {
                // Brief 059 (design 059 §6): `cable-not-drawn` covers a cable not drawn yet.
                unknown += usize::from(r.drawn);
                for i in &r.count.twice {
                    out.push(self.core_twice(r, &tag, *i));
                }
                continue;
            };
            if let Some(first) = r.count.over.first().and_then(|i| r.members.get(*i)) {
                let go = self.wire(*first).and_then(|w| w.go);
                let named = match (&r.kind, &r.count.offer_text) {
                    (Some(k), _) => k.clone(),
                    (None, Some(t)) => t.clone(),
                    (None, None) => String::new(),
                };
                out.push(Check {
                    kind: Kind::TooManyCores,
                    page: go.map(|(p, _)| p),
                    at: go.map(|(_, a)| a),
                    object: Object::Conductor(*first),
                    which: format!("{tag}: {} cores used, cable offers {} ({named})", r.members.len(), offer.len()),
                    fix: Some(Fix::SetCores(r.cable)),
                });
            }
            for i in &r.count.twice {
                out.push(self.core_twice(r, &tag, *i));
            }
        }
        if unknown > 0 {
            let which = if unknown == 1 { "1 cable has no core count".to_string() } else { format!("{unknown} cables have no core count") };
            out.push(Check { kind: Kind::NoCoreCount, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowCables) });
        }
        out
    }

    fn core_twice(&self, r: &CableRow, tag: &str, i: usize) -> Check {
        let w = r.members.get(i).copied();
        let go = w.and_then(|w| self.wire(w)).and_then(|x| x.go);
        let label = w.and_then(|w| self.wire(w)).map(|x| x.cable.clone()).unwrap_or_default();
        let core = label.rsplit(" core ").next().unwrap_or("?").to_string();
        Check {
            kind: Kind::CoreTwice,
            page: go.map(|(p, _)| p),
            at: go.map(|(_, a)| a),
            object: w.map_or(Object::None, Object::Conductor),
            which: format!("{tag}: core {core} is used by two wires"),
            fix: w.map(Fix::NextFreeCore),
        }
    }

    /// Brief 047 (design 047 §9): the BOM rows of the Checks list, from the kept counts and sets.
    pub fn bom_checks(&self) -> Vec<Check> {
        let mut out = Vec::new();
        let n = self.items_no_part;
        if n > 0 {
            let which = if n == 1 { "1 item has no part number".to_string() } else { format!("{n} items have no part number") };
            out.push(Check { kind: Kind::ItemNoPartNumber, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        let gaps = self.fuse_gap_count();
        if gaps > 0 {
            let which = if gaps == 1 { "1 fuse has no insert".to_string() } else { format!("{gaps} fuses have no insert") };
            out.push(Check { kind: Kind::FuseNoInsert, page: None, at: None, object: Object::None, which, fix: Some(Fix::ShowReports) });
        }
        for pk in &self.article_conflicts {
            let Some(set) = self.articles.get(pk) else { continue };
            let Some(first) = self.by_part.get(pk).and_then(|s| s.iter().next()).and_then(|(_, _, c, n)| self.entries.get(&(*c, *n))) else {
                continue;
            };
            let mut values: Vec<(&String, &usize)> = set.iter().collect();
            values.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let words: Vec<String> = values.iter().map(|(a, n)| format!("{a} ({n})")).collect();
            let pn = first.part_number.clone().unwrap_or_default();
            let count = if values.len() == 2 { "two".to_string() } else { values.len().to_string() };
            out.push(Check {
                kind: Kind::ArticleConflict,
                page: None,
                at: None,
                object: Object::Component(first.component),
                which: format!("{pn} has {count} article numbers: {}", words.join(", ")),
                fix: Some(Fix::UseArticle(first.component, first.line)),
            });
        }
        for (c, n) in &self.for_missing {
            let Some(e) = self.entries.get(&(*c, *n)) else { continue };
            let main = self.devices.get(c).and_then(|d| d.goto);
            let what = if self.strips.get(*c).is_some() { "position" } else { "fuse" };
            let names = e.missing.join(", ");
            let plural = if e.missing.len() == 1 { "" } else { "s" };
            out.push(Check {
                kind: Kind::PartForMissing,
                page: main.map(|(_, p, _)| p),
                at: main.and_then(|(_, _, a)| a),
                object: Object::Component(*c),
                which: format!("{} line {n} names {what}{plural} {names}, which {} does not have", e.tag, e.tag),
                fix: Some(Fix::DropMissingFor(*c, *n)),
            });
        }
        for (c, keys) in &self.bad_part_keys {
            let tag = self.devices.get(c).map_or_else(|| "?".to_string(), |d| d.tag.clone());
            let main = self.devices.get(c).and_then(|d| d.goto);
            let first = keys.first().cloned().unwrap_or_default();
            let more = if keys.len() > 1 { format!(" (and {} more)", keys.len() - 1) } else { String::new() };
            let which = match self.entries.get(&(*c, 0)).filter(|e| e.item) {
                Some(e) => {
                    format!("The item {} has {first}{more}, which ECAD does not read; it counts as 1 piece", e.description.as_deref().unwrap_or("?"))
                }
                None => format!("{tag} has {first}{more}, which ECAD does not read"),
            };
            out.push(Check {
                kind: Kind::BadPartKey,
                page: main.map(|(_, p, _)| p),
                at: main.and_then(|(_, _, a)| a),
                object: Object::Component(*c),
                which,
                fix: None,
            });
        }
        out
    }

    /// Uncovered fuses, counted only once the project lists any fuse insert (design 047 §9, Q11).
    fn fuse_gap_count(&self) -> usize {
        if self.inserts.is_empty() { 0 } else { self.fuse_gaps.values().sum() }
    }

    /// The check rows by severity: (errors, warnings, info), as [`Outputs::checks`] would give
    /// them. O(1): kept by every change (brief 027's badge).
    pub fn check_counts(&self) -> (usize, usize, usize) {
        let info = self.box_counts[1]
            + usize::from(self.strips.acc_no_part > 0)
            + usize::from(self.no_part_number > 0 || self.lines_no_part > 0)
            + usize::from(self.items_no_part > 0)
            + usize::from(self.fuse_gap_count() > 0)
            + self.bad_part_keys.len()
            + usize::from(self.no_location > 0 && self.no_location < self.devices.len())
            + usize::from(self.no_function > 0 && self.no_function < self.devices.len())
            + usize::from(self.cable_checks[2] > 0);
        // Brief 028: the strips' and cables' rows, kept with them.
        let warnings = self.box_counts[0]
            + self.strips.check_warnings
            + self.cable_checks[0]
            + self.cable_checks[1]
            + self.links_off_strip.len()
            + self.article_conflicts.len()
            + self.for_missing.len();
        (self.strips.check_errors, warnings, info)
    }

    /// What the check rows depend on besides [`Outputs::checks_changed`]: the three count rows.
    pub fn checks_key(&self) -> (u64, usize, usize, usize, usize) {
        (self.checks_changed, self.no_part_number, self.no_location, self.no_function, self.devices.len())
    }

    /// How many sheets the BOM-by-location page needs (design 024 §8.2): from the rows per
    /// location only (O(locations)), never the BOM rows themselves.
    pub fn bom_sheets(&self) -> usize {
        let per = crate::generated::rows_per_sheet();
        let (mut sheets, mut used) = (1, 0);
        // Brief 028: the strips' rows per location, kept with them (O(locations), never the rows).
        // Brief 047: the nested layout's counts are kept too (main rows, then each part line).
        let mut rows = if self.view.nested {
            let mut r = self.loc_nested_rows.clone();
            for (k, n) in &self.loc_lines {
                *r.entry(k.clone()).or_default() += n;
            }
            r
        } else {
            self.loc_rows.clone()
        };
        for (loc, n) in &self.strips.bom_rows {
            *rows.entry(loc_key(loc.as_deref())).or_default() += n;
        }
        // Brief 044: the connectors' half-row groups per location, kept.
        for (k, n) in &self.half_loc_rows {
            *rows.entry(k.clone()).or_default() += n;
        }
        for n in rows.values() {
            if used == per {
                sheets += 1;
                used = 0;
            }
            used += 1;
            let mut left = *n;
            while left > 0 {
                if used == per {
                    sheets += 1;
                    used = 1;
                }
                let take = left.min(per - used);
                used += take;
                left -= take;
            }
        }
        sheets
    }

    /// The `link.<n>` keys a device that is not a strip carries (kept per device, read on demand).
    pub fn link_keys_off_strip(&self, c: ComponentId) -> Vec<String> {
        if !self.links_off_strip.contains(&c) {
            return Vec::new();
        }
        self.link_keys.get(&c).cloned().unwrap_or_default()
    }

    /// One device's row.
    pub fn device(&self, c: ComponentId) -> Option<&DeviceRow> {
        self.devices.get(&c)
    }

    /// How many devices.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// A strip's BOM rows (design 028 §9.1, Q13; brief 036 §7.2): one row per terminal type, Qty
    /// in positions (a three-level terminal is one part), in catalogue order.
    fn strip_bom(&self, s: &crate::strips::StripData, location: Option<String>) -> Vec<BomRow> {
        let mut out = Vec::new();
        for t in &s.types {
            if t.qty() == 0 {
                continue;
            }
            out.push(BomRow {
                location: location.clone(),
                qty: Qty::count(t.qty()),
                tags: vec![s.tag.clone()],
                devices: vec![s.component],
                description: Some(t.description.clone()),
                maker: t.maker.clone(),
                part_number: t.part_number.clone(),
                article_number: t.article_number.clone(),
                ..BomRow::default()
            });
        }
        // Brief 030 (design 030 §7.4): one row per accessory part, only for what is placed.
        for a in &s.acc.bom {
            out.push(BomRow {
                location: location.clone(),
                qty: Qty::count(a.qty),
                tags: vec![s.tag.clone()],
                devices: vec![s.component],
                description: Some(a.description.clone()),
                maker: a.maker.clone(),
                part_number: a.part_number.clone(),
                article_number: a.article_number.clone(),
                accessory: Some(a.accessory),
                ..BomRow::default()
            });
        }
        out
    }

    /// Brief 047: the row a set of entries makes (design 047 §4.2, §4.3).
    fn entry_row(&self, set: &BTreeSet<EntrySort>, location: Option<String>) -> BomRow {
        let es: Vec<&Entry> = set.iter().filter_map(|(_, _, c, n)| self.entries.get(&(*c, *n))).collect();
        let Some(first) = es.first() else { return BomRow { location, ..BomRow::default() } };
        let (mut qty, mut overflow) = (Qty(0), false);
        for e in &es {
            match e.total().and_then(|t| qty.checked_add(t)) {
                Some(q) => qty = q,
                None => overflow = true,
            }
        }
        let mut tags: Vec<String> = es.iter().flat_map(|e| e.tags()).collect();
        if es.iter().any(|e| e.per_sub()) {
            tags.sort_by(|a, b| locations::natural_key(a).cmp(&locations::natural_key(b)).then(a.cmp(b)));
        }
        tags.dedup();
        let mut devices: Vec<ComponentId> = Vec::new();
        for e in &es {
            if !devices.contains(&e.component) {
                devices.push(e.component);
            }
        }
        // The most common article number (ties: the first in tag order); flagged when two differ.
        let mut counts: Vec<(&str, usize, &Entry)> = Vec::new();
        for e in &es {
            let Some(a) = e.article_number.as_deref() else { continue };
            match counts.iter_mut().find(|(x, _, _)| *x == a) {
                Some(c) => c.1 += 1,
                None => counts.push((a, 1, e)),
            }
        }
        let best = counts.iter().enumerate().max_by(|(i, a), (j, b)| a.1.cmp(&b.1).then(j.cmp(i))).map(|(_, x)| *x);
        let article_flag = match (best, counts.iter().find(|x| Some(x.0) != best.map(|b| b.0))) {
            (Some((a, _, ea)), Some((b, _, eb))) => {
                let who = |e: &Entry| if e.item { e.description.clone().unwrap_or_else(|| "An item".into()) } else { e.tag.clone() };
                Some(format!("{} and {} have different article numbers ({a}, {b}).", who(ea), who(eb)))
            }
            _ => None,
        };
        BomRow {
            location,
            qty,
            metres: first.metres,
            overflow,
            tags,
            description: first.description.clone(),
            maker: first.maker.clone(),
            part_number: first.part_number.clone(),
            article_number: best.map(|b| b.0.to_string()),
            article_flag,
            entries: es.iter().map(|e| (e.component, e.line)).collect(),
            item: es.iter().all(|e| e.item),
            devices,
            ..BomRow::default()
        }
    }

    /// Brief 047, nested layout: the rows of `c`'s part lines, unmerged, in line order.
    /// Brief 044: the part lines of connector `c` under its half tagged `tag` (nested layout).
    fn line_rows_of_half(&self, c: ComponentId, tag: &str, location: &Option<String>) -> Vec<BomRow> {
        let mine: BTreeSet<u8> = self.entries.range((c, 1)..=(c, u8::MAX)).filter(|(_, e)| e.tag == tag).map(|(k, _)| k.1).collect();
        self.line_rows(c, location).into_iter().filter(|r| r.line.is_some_and(|n| mine.contains(&n))).collect()
    }

    fn line_rows(&self, c: ComponentId, location: &Option<String>) -> Vec<BomRow> {
        self.entries
            .range((c, 1)..=(c, u8::MAX))
            .map(|(_, e)| {
                let (qty, overflow) = e.total().map_or((e.qty, true), |q| (q, false));
                BomRow {
                    location: location.clone(),
                    qty,
                    metres: e.metres,
                    overflow,
                    tags: if e.per_sub() { e.tags() } else { Vec::new() },
                    devices: vec![c],
                    description: e.description.clone(),
                    maker: e.maker.clone(),
                    part_number: e.part_number.clone(),
                    article_number: e.article_number.clone(),
                    entries: vec![(c, e.line)],
                    line: Some(e.line),
                    kit: e.kit.clone(),
                    ..BomRow::default()
                }
            })
            .collect()
    }

    /// The BOM rows (design 024 §6.1): devices, and each strip per terminal type (brief 028);
    /// brief 047: part lines and items, in the project's layout (`bom.lines`).
    pub fn bom(&self, group: Group) -> Vec<BomRow> {
        self.bom_with(group, self.view.nested)
    }

    /// Brief 050b (design 050 §5.6): one wire row per distinct (part number, colour,
    /// cross-section) among wires whose category has a part number; Qty in metres, the wires ×
    /// the category's allowance rounded up to 0.5 m (an estimate); with no allowance, no Qty.
    pub fn wire_bom(&self) -> Vec<BomRow> {
        let mut groups: BTreeMap<(String, String, String), (usize, f64, Option<String>, Option<String>)> = BTreeMap::new();
        for w in self.wires.values() {
            let Some((pn, maker, description, allowance)) = &w.part else { continue };
            let key = (pn.clone(), w.colour.clone().unwrap_or_default(), w.cross_section.clone().unwrap_or_default());
            let g = groups.entry(key).or_insert((0, *allowance, maker.clone(), description.clone()));
            g.0 += 1;
        }
        groups
            .into_iter()
            .map(|((pn, colour, cs), (n, allowance, maker, description))| {
                let metres = (n as f64 * allowance * 2.0).ceil() / 2.0;
                let qty = if metres.is_finite() && metres > 0.0 { Qty((metres * 1000.0).round().min(u64::MAX as f64 / 2.0) as u64) } else { Qty(0) };
                let what = [
                    Some(description.unwrap_or_else(|| "Wire".into())),
                    (!cs.is_empty()).then_some(cs),
                    (!colour.is_empty()).then(|| ecad_model::wiring::colour_text(self.colour_style, &colour)),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
                let note = if qty.0 == 0 { format!("{n} wires, no allowance") } else { format!("{n} wires, estimate") };
                BomRow {
                    location: None,
                    qty,
                    metres: true,
                    overflow: false,
                    tags: Vec::new(),
                    devices: Vec::new(),
                    description: Some(format!("{what} ({note})")),
                    maker,
                    part_number: Some(pn),
                    article_number: None,
                    article_flag: None,
                    entries: Vec::new(),
                    item: true,
                    line: None,
                    kit: None,
                    accessory: None,
                }
            })
            .collect()
    }

    /// The BOM rows in the flat or nested layout (nested is By location only).
    pub fn bom_with(&self, group: Group, nested: bool) -> Vec<BomRow> {
        let mut rows = self.bom_rows_with(group, nested);
        rows.extend(self.wire_bom());
        rows
    }

    fn bom_rows_with(&self, group: Group, nested: bool) -> Vec<BomRow> {
        match group {
            Group::Location => {
                let index = if nested { &self.nested } else { &self.by_location };
                let mut v: Vec<(LocKey, EntrySort, Vec<BomRow>)> = index
                    .iter()
                    .filter_map(|((lk, pk), set)| {
                        let first = set.iter().next()?.clone();
                        let loc = (!lk.0).then(|| lk.2.clone());
                        let mut rows = vec![self.entry_row(set, loc.clone())];
                        // A device row of its own is followed by its lines.
                        if nested && let Some((c, 0)) = pk.3 {
                            rows.extend(self.line_rows(c, &loc));
                        }
                        Some((lk.clone(), first, rows))
                    })
                    .collect();
                for s in self.strips.strips.values() {
                    let mut rows = self.strip_bom(s, s.location.clone());
                    if nested {
                        rows.extend(self.line_rows(s.component, &s.location));
                    }
                    v.push((loc_key(s.location.as_deref()), (false, tag_key(&s.tag), s.component, 0), rows));
                }
                // Brief 044 (design 044 §8.1): one row per drawn half, grouped like devices; nested,
                // each half's row is followed by its part lines (`part.<n>.half`, §8.3).
                self.half_groupings.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                for r in group_halves(self.connectors.values().flatten(), Group::Location) {
                    let first = r.tags.first().map_or_else(|| tag_key(""), |t| tag_key(t));
                    let sort = (false, first, r.devices.first().copied().unwrap_or(ComponentId(0)), 0);
                    let mut rows = Vec::new();
                    if nested && r.part_number.is_none() {
                        for (c, tag) in r.devices.iter().zip(&r.tags) {
                            rows.extend(self.line_rows_of_half(*c, tag, &r.location));
                        }
                    }
                    rows.insert(0, r.clone());
                    v.push((loc_key(r.location.as_deref()), sort, rows));
                }
                v.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                v.into_iter().flat_map(|(_, _, r)| r).collect()
            }
            Group::Part => {
                let mut v: Vec<(EntrySort, BomRow)> = self
                    .by_part
                    .values()
                    .filter_map(|set| {
                        let first = set.iter().next()?.clone();
                        let mut counts: BTreeMap<LocKey, usize> = BTreeMap::new();
                        for (_, _, c, n) in set {
                            if let Some(e) = self.entries.get(&(*c, *n)) {
                                *counts.entry(e.loc_key()).or_default() += 1;
                            }
                        }
                        let locs: Vec<String> =
                            counts.iter().map(|(k, n)| format!("{} \u{d7}{n}", if k.0 { "?".to_string() } else { k.2.clone() })).collect();
                        Some((first, self.entry_row(set, Some(locs.join(", ")))))
                    })
                    .collect();
                for s in self.strips.strips.values() {
                    let loc = s.location.clone().unwrap_or_else(|| "?".into());
                    for r in self.strip_bom(s, Some(format!("{loc} \u{d7}1"))) {
                        v.push(((false, tag_key(&s.tag), s.component, 0), r));
                    }
                }
                // Brief 044: half rows grouped by part, their locations counted.
                self.half_groupings.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                for r in group_halves(self.connectors.values().flatten(), Group::Part) {
                    let first = r.tags.first().map_or_else(|| tag_key(""), |t| tag_key(t));
                    v.push(((false, first, r.devices.first().copied().unwrap_or(ComponentId(0)), 0), r));
                }
                v.sort_by(|a, b| a.0.cmp(&b.0));
                v.into_iter().map(|(_, r)| r).collect()
            }
        }
    }

    /// Brief 047: one BOM entry.
    pub fn entry(&self, c: ComponentId, line: u8) -> Option<&Entry> {
        self.entries.get(&(c, line))
    }

    /// Brief 047: the entries of `c` (main part or item first, then its part lines).
    pub fn entries_of(&self, c: ComponentId) -> impl Iterator<Item = &Entry> {
        self.entries.range((c, 0)..=(c, u8::MAX)).map(|(_, e)| e)
    }

    /// Brief 047: the article numbers of part key of entry `(c, line)`, most common first.
    pub fn article_values(&self, c: ComponentId, line: u8) -> Vec<(String, usize)> {
        let Some(pk) = self.entries.get(&(c, line)).map(Entry::part_key) else { return Vec::new() };
        let mut v: Vec<(String, usize)> = self.articles.get(&pk).map(|m| m.iter().map(|(a, n)| (a.clone(), *n)).collect()).unwrap_or_default();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    /// The wire rows, sorted by wire number naturally, then conductor ID.
    pub fn wires(&self) -> Vec<&WireRow> {
        let mut v: Vec<&WireRow> = self.wires.values().collect();
        v.sort_by(|a, b| {
            let ka = loc_key(a.wire_number.as_deref());
            let kb = loc_key(b.wire_number.as_deref());
            ka.cmp(&kb).then(a.conductor.cmp(&b.conductor))
        });
        v
    }

    /// The cable rows, sorted by tag naturally.
    pub fn cables(&self) -> Vec<&CableRow> {
        self.sorts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut v: Vec<&CableRow> = self.cables.values().collect();
        v.sort_by(|a, b| loc_key(a.tag.as_deref()).cmp(&loc_key(b.tag.as_deref())).then(a.cable.cmp(&b.cable)));
        v
    }

    /// One wire's row.
    pub fn wire(&self, w: ConductorId) -> Option<&WireRow> {
        self.wires.get(&w)
    }

    /// The BOM as a table (`Location` or `Locations` first), as the window shows it, in the
    /// project's report settings.
    pub fn bom_table(&self, group: Group) -> Table {
        self.bom_table_with(group, self.view, false)
    }

    /// The BOM as a table in `view`. `csv` (design 047 §6.4): `Qty` a plain number and a `Unit`
    /// column, a `Line` column in the nested layout; otherwise `Qty` reads `5.6 m`.
    pub fn bom_table_with(&self, group: Group, view: BomView, csv: bool) -> Table {
        let first = if group == Group::Location { "Location" } else { "Locations" };
        let nested = view.nested && group == Group::Location;
        let mut headers = vec![first, "Qty"];
        if csv {
            headers.push("Unit");
        }
        headers.push("Tags");
        if csv && nested {
            headers.push("Line");
        }
        headers.extend(["Description", "Maker", "Part number"]);
        if view.article {
            headers.push("Article number");
        }
        let mut t = Table { headers, rows: Vec::new() };
        for r in self.bom_with(group, nested) {
            let who = self.row_who(&r);
            let mut cells = vec![Cell::opt(&r.location, || format!("{who} has no location."))];
            let qty = if r.no_qty() {
                String::new()
            } else if csv {
                r.qty.to_string()
            } else {
                r.qty_text()
            };
            cells.push(if r.overflow { Cell { text: qty, flag: Some("The quantity is too large to add up.".into()) } } else { Cell::ok(qty) });
            if csv {
                cells.push(Cell::ok(if r.metres { "m" } else { "pcs" }));
            }
            cells.push(Cell::ok(r.tags.join(", ")));
            if csv && nested {
                // `0` a main part (or a strip's own row), `1`… a line, blank for an item.
                let line = match (r.line, r.item) {
                    (Some(n), _) => n.to_string(),
                    (None, true) => String::new(),
                    (None, false) => "0".into(),
                };
                cells.push(Cell::ok(line));
            }
            let description = if csv { r.description.clone() } else { r.description_text() };
            cells.push(Cell::opt(&description, || format!("{who} has no description.")));
            cells.push(Cell::opt(&r.maker, || format!("{who} has no maker.")));
            cells.push(Cell::opt(&r.part_number, || format!("{who} has no part number.")));
            if view.article {
                let a = r.article_number.clone().unwrap_or_default();
                cells.push(match &r.article_flag {
                    Some(why) => Cell { text: if a.is_empty() { "?".into() } else { a }, flag: Some(why.clone()) },
                    None => Cell::ok(a),
                });
            }
            t.rows.push(cells);
        }
        t
    }

    /// Who a row's `?` hover names: the first tag, `Line 2 of -K03` for a nested line, or the item.
    fn row_who(&self, r: &BomRow) -> String {
        if let (Some(n), Some(c)) = (r.line, r.devices.first()) {
            let tag = self.devices.get(c).map_or("?", |d| d.tag.as_str());
            return format!("Line {n} of {tag}");
        }
        match r.tags.first() {
            Some(t) => t.clone(),
            None if r.item => format!("The item {}", r.description.as_deref().unwrap_or("")).trim_end().to_string(),
            None => String::new(),
        }
    }

    /// The wire report as a table.
    pub fn wire_table(&self) -> Table {
        let mut t = Table {
            headers: vec![
                "Wire number",
                "From",
                "From location",
                "To",
                "To location",
                "Cross-section",
                "Colour",
                "Potential",
                "Category",
                "Cable",
                "Page",
            ],
            rows: Vec::new(),
        };
        for r in self.wires() {
            let style = self.style;
            let end_loc = |e: &EndCell| match (&e.device, &e.location) {
                (Some(_), None) => Cell { text: "?".into(), flag: Some(format!("{} has no location.", e.text(style).unwrap_or_default())) },
                (_, Some(l)) => Cell::ok(l.clone()),
                (None, None) => Cell::default(),
            };
            t.rows.push(vec![
                Cell::opt(&r.wire_number, || "The wire has no number.".into()),
                Cell::opt(&r.from.text(style), || "Loose end.".into()),
                end_loc(&r.from),
                Cell::opt(&r.to.text(style), || "Loose end.".into()),
                end_loc(&r.to),
                // Brief 050b (design 050 §5.6): the wire's own value or its category's.
                Cell::opt(&r.cross_section, || "No cross-section, and its category gives none.".into()),
                Cell::opt(&r.colour.as_deref().map(|c| ecad_model::wiring::colour_text(self.colour_style, c)), || {
                    "No colour, and its category gives none.".into()
                }),
                Cell::ok(r.potential.clone().unwrap_or_default()),
                Cell::ok(r.category.as_ref().map(|c| c.1.clone()).unwrap_or_default()),
                Cell::ok(r.cable.clone()),
                Cell::opt(&r.page.as_ref().map(|p| p.text(style)), || "The wire has no route.".into()),
            ]);
        }
        t
    }

    /// The cable report as a table; `Maker`, `Length` and `Designation` follow when any cable
    /// has them. Brief 059: cables not drawn are listed (design 059 §4.5, Q3); see
    /// [`Outputs::cable_table_with`].
    pub fn cable_table(&self) -> Table {
        self.cable_table_with(true)
    }

    /// The cable report, with or without the cables not drawn yet (brief 059, Q3: the choice made
    /// when the report is generated). `Glands`, `Shield`, `Planned from` and `Planned to` follow
    /// when any listed cable has them, and `Drawn` when one is not drawn; a planned end reads
    /// `(planned)`.
    pub fn cable_table_with(&self, include_not_drawn: bool) -> Table {
        let mut rows = self.cables();
        if !include_not_drawn {
            rows.retain(|r| r.drawn);
        }
        let any = |f: fn(&CableRow) -> &Option<String>| rows.iter().any(|r| f(r).is_some());
        let extra: Vec<(&'static str, fn(&CableRow) -> &Option<String>)> = [
            ("Maker", (|r: &CableRow| &r.maker) as fn(&CableRow) -> &Option<String>),
            ("Length", |r| &r.length),
            ("Designation", |r| &r.designation),
        ]
        .into_iter()
        .filter(|(_, f)| any(*f))
        .collect();
        let mut t = Table { headers: vec!["Tag", "Type", "Cores", "From location", "To location", "Used", "Spare", "From", "To"], rows: Vec::new() };
        t.headers.extend(extra.iter().map(|(h, _)| *h));
        // Brief 059 (design 059 §4.5): the manager's columns, when any listed cable has them.
        let glands = rows.iter().any(|r| r.glands.iter().any(Option::is_some));
        let shield = rows.iter().any(|r| r.shield.is_some());
        let planned = rows.iter().any(|r| r.planned.iter().any(Option::is_some));
        let drawn_col = rows.iter().any(|r| !r.drawn);
        if glands {
            t.headers.push("Glands");
        }
        if shield {
            t.headers.push("Shield");
        }
        if planned {
            t.headers.extend(["Planned from", "Planned to"]);
        }
        if drawn_col {
            t.headers.push("Drawn");
        }
        for r in rows {
            let why = r.location_reason.clone();
            let loc = |v: &Option<String>| match v {
                Some(l) => Cell::ok(l.clone()),
                None => Cell { text: "?".into(), flag: Some(why.clone().unwrap_or_default()) },
            };
            let mut row = vec![
                Cell::opt(&r.tag, || "The cable has no tag.".into()),
                Cell::opt(&r.kind, || "The cable has no type.".into()),
                Cell::ok(r.core_total().to_string()),
                loc(&r.from_location),
                loc(&r.to_location),
                Cell::ok(r.count.used().to_string()),
                Cell::opt(&r.count.spare_text(), || "Set the cable's cores or type (7G1.5) to count spares.".into()),
                Cell::ok(ends_text(&r.from_location, &r.from_devices)),
                Cell::ok(ends_text(&r.to_location, &r.to_devices)),
            ];
            for (_, f) in &extra {
                row.push(Cell::ok(f(r).clone().unwrap_or_default()));
            }
            let dash = |v: &Option<String>| v.clone().unwrap_or_else(|| "\u{2014}".into());
            if glands {
                row.push(Cell::ok(format!("{}, {}", dash(&r.glands[0]), dash(&r.glands[1]))));
            }
            if shield {
                row.push(Cell::ok(r.shield.clone().unwrap_or_default()));
            }
            if planned {
                for p in &r.planned {
                    row.push(Cell::ok(p.as_ref().map(|p| format!("{p} (planned)")).unwrap_or_default()));
                }
            }
            if drawn_col {
                row.push(Cell::ok(if r.drawn { "yes" } else { "no" }));
            }
            t.rows.push(row);
        }
        t
    }
}

impl Outputs {
    /// The terminal plan as a table (Reports ▸ Terminals, CSV): every strip, or `strip` only;
    /// one line per position and conductor (design 028 §5.1), with where each line goes.
    pub fn terminal_table(&self, strip: Option<ComponentId>) -> (Table, Vec<Option<(PageId, Pt)>>) {
        self.terminal_table_with(strip, true)
    }

    /// [`Outputs::terminal_table`], with or without the accessory gap rows (brief 030: the
    /// Terminals tab's Accessories check box; view only).
    pub fn terminal_table_with(&self, strip: Option<ComponentId>, accessories: bool) -> (Table, Vec<Option<(PageId, Pt)>>) {
        let style = self.style;
        let mut t = Table {
            headers: vec!["Strip", "Inside target", "Inside wire", "Terminal", "Bridge", "Outside wire", "Cable", "Core", "Outside target", "Place"],
            rows: Vec::new(),
        };
        let mut go = Vec::new();
        let strips: Vec<&crate::strips::StripData> = match strip {
            Some(c) => self.strips.get(c).into_iter().collect(),
            None => self.strips.sorted(),
        };
        for s in strips {
            for item in s.plan(accessories) {
                let (ri, r) = match item {
                    crate::strips::PlanItem::Term(i, r) => (i, r),
                    crate::strips::PlanItem::Gap { gap, acc } => {
                        let pe_before = gap_pe(s, gap);
                        let words =
                            if acc == crate::accessories::Acc::StripLabel { format!("strip label {}", s.tag) } else { acc.words().to_string() };
                        let mut row = vec![Cell::ok(s.tag.clone()), Cell::ok(words), Cell::default(), Cell::ok(acc.mark())];
                        row.extend(std::iter::repeat_n(Cell::default(), 5));
                        row.push(Cell::opt(&s.acc.gap_part(acc, pe_before).cloned(), || "No part number for this accessory.".into()));
                        t.rows.push(row);
                        go.push(None);
                        continue;
                    }
                };
                let marker = if s.acc.markers.contains(&r.label) { " \u{25ad}" } else { "" };
                // Brief 036: a PE mark only on a one-level PE terminal (an L/N/PE level shows `1.PE`).
                let pe_mark = r.pe && !r.ty.multi();
                if r.terminal.is_none() {
                    let mut row = vec![Cell::ok(s.tag.clone()), Cell::ok(r.state()), Cell::default()];
                    row.push(Cell::ok(if pe_mark { format!("{} PE{marker}", r.shown) } else { format!("{}{marker}", r.shown) }));
                    row.push(Cell::ok(s.bridge_cell(ri)));
                    row.extend(std::iter::repeat_n(Cell::default(), 5));
                    t.rows.push(row);
                    go.push(None);
                    continue;
                }
                let lines = r.inside.len().max(r.outside.len()).max(1);
                for i in 0..lines {
                    let (inside, outside) = (r.inside.get(i), r.outside.get(i));
                    let side = |c: Option<&crate::strips::SideCell>, f: fn(&crate::strips::SideCell) -> Option<String>, why: &str| match c {
                        None => Cell::default(),
                        Some(c) => Cell::opt(&f(c), || why.to_string()),
                    };
                    let label = if i == 0 { format!("{}{marker}", r.shown) } else { format!("{}\u{2032}", r.shown) };
                    let cable = outside.and_then(|c| c.cable.as_ref());
                    t.rows.push(vec![
                        Cell::ok(s.tag.clone()),
                        side(inside, |c| c.target.clone(), "A loose far end."),
                        side(inside, |c| c.wire.clone(), "The wire has no number."),
                        Cell::ok(if pe_mark { format!("{label} PE") } else { label }),
                        Cell::ok(if i == 0 { s.bridge_cell(ri) } else { String::new() }),
                        side(outside, |c| c.wire.clone(), "The wire has no number."),
                        Cell::ok(cable.map_or_else(String::new, |(_, t)| t.clone().unwrap_or_else(|| "?".into()))),
                        match (cable, outside) {
                            (Some(_), Some(c)) => Cell::opt(&c.core, || "The core has no label.".into()),
                            _ => Cell::default(),
                        },
                        side(outside, |c| c.target.clone(), "A loose far end."),
                        Cell::ok(if i == 0 { r.place.as_ref().map(|p| p.text(style)).unwrap_or_default() } else { String::new() }),
                    ]);
                    go.push(r.go);
                }
            }
        }
        (t, go)
    }

    /// The terminal plan as the release writes it (027's `terminal-plan` slot): one line per
    /// position and conductor of every strip in tag order, columns named by `stripplan`'s keys
    /// (design 028 §10); an absent value is an empty cell.
    pub fn terminal_plan_csv(&self) -> Table {
        let style = self.style;
        let mut t = Table {
            headers: vec![
                "tag",
                "location",
                "offered",
                "label",
                "type",
                "state",
                "inside_target",
                "inside_wire",
                "bridge_group",
                "outside_wire",
                "cable",
                "core",
                "outside_target",
                "place",
                // Brief 030 (design 030 §9.3).
                "kind",
                "accessory",
                "gap",
                "marker",
                "links",
            ],
            rows: Vec::new(),
        };
        let opt = |v: Option<&String>| Cell::ok(v.cloned().unwrap_or_default());
        for s in self.strips.sorted() {
            for item in s.plan(true) {
                let r = match item {
                    crate::strips::PlanItem::Term(_, r) => r,
                    crate::strips::PlanItem::Gap { gap, acc } => {
                        let mut row =
                            vec![Cell::ok(s.tag.clone()), opt(s.location.as_ref()), Cell::ok(s.offer.map(|o| o.text()).unwrap_or_default())];
                        row.extend([Cell::default(), Cell::default(), Cell::ok("accessory")]);
                        row.extend(std::iter::repeat_n(Cell::default(), 7));
                        row.push(Cell::ok(s.acc.gap_part(acc, gap_pe(s, gap)).cloned().unwrap_or_else(|| "?".into())));
                        row.extend([Cell::ok("accessory"), Cell::ok(acc.key()), Cell::ok(gap), Cell::default(), Cell::default()]);
                        t.rows.push(row);
                        continue;
                    }
                };
                let marker = if s.acc.markers.contains(&r.label) { "yes" } else { "" };
                let links: Vec<&str> = s.acc.links.iter().filter(|l| l.labels.contains(&r.label)).map(|l| l.text.as_str()).collect();
                let lines = r.inside.len().max(r.outside.len()).max(1);
                for i in 0..lines {
                    let (inside, outside) = (r.inside.get(i), r.outside.get(i));
                    let label = if i == 0 { r.shown.clone() } else { format!("{}\u{2032}", r.shown) };
                    t.rows.push(vec![
                        Cell::ok(s.tag.clone()),
                        opt(s.location.as_ref()),
                        Cell::ok(s.offer.map(|o| o.text()).unwrap_or_default()),
                        Cell::ok(label),
                        Cell::ok(match r.ty.code {
                            "ft" => "feed-through",
                            "pe" => "pe",
                            c => c,
                        }),
                        Cell::ok(r.state()),
                        opt(inside.and_then(|c| c.target.as_ref())),
                        opt(inside.and_then(|c| c.wire.as_ref())),
                        Cell::ok(r.bridge_group.filter(|_| i == 0).map(|g| g.to_string()).unwrap_or_default()),
                        opt(outside.and_then(|c| c.wire.as_ref())),
                        Cell::ok(outside.and_then(|c| c.cable.as_ref()).map(|(_, t)| t.clone().unwrap_or_else(|| "?".into())).unwrap_or_default()),
                        opt(outside.and_then(|c| c.core.as_ref())),
                        opt(outside.and_then(|c| c.target.as_ref())),
                        Cell::ok(if i == 0 { r.place.as_ref().map(|p| p.text(style)).unwrap_or_default() } else { String::new() }),
                        Cell::ok("terminal"),
                        Cell::default(),
                        Cell::default(),
                        Cell::ok(marker),
                        Cell::ok(links.join("; ")),
                    ]);
                }
            }
        }
        t
    }

    /// The cable schedule as the release writes it (027's `cable-schedule` slot): every cable in
    /// tag order with its cores (`cablecores`' keys: `label`, `state`, `wire`, `from`, `to`), an
    /// over core after the offered ones; an absent value is an empty cell.
    pub fn cable_schedule_csv(&self) -> Table {
        let mut t = Table { headers: vec!["cable", "type", "offer", "label", "state", "wire", "from", "to"], rows: Vec::new() };
        for r in self.cables() {
            let head = [
                Cell::ok(r.tag.clone().unwrap_or_default()),
                Cell::ok(r.kind.clone().unwrap_or_default()),
                Cell::ok(r.count.offer_text.clone().unwrap_or_default()),
            ];
            for row in self.core_table(r.cable).rows {
                let mut line = head.to_vec();
                // core_table: Core, Wire number, From, To, State; a spare core has no From.
                let get = |i: usize| row.get(i).map(|c| c.text.clone()).unwrap_or_default();
                let state = get(4);
                let from = if state == "spare" { String::new() } else { get(2) };
                line.extend([Cell::ok(get(0)), Cell::ok(state), Cell::ok(get(1)), Cell::ok(from), Cell::ok(get(3))]);
                t.rows.push(line);
            }
        }
        t
    }

    /// Brief 032 §3.5: the From and To of core `i` of cable row `r` (a core chain runs from its
    /// first member's outer end to its last member's), in reference style `style`.
    pub fn chain_ends(&self, r: &CableRow, i: usize, style: ecad_model::ReferenceStyle) -> (Option<String>, Option<String>) {
        let w = r.members.get(i).and_then(|w| self.wires.get(w));
        let tail = r.tails.get(i).filter(|t| Some(**t) != r.members.get(i).copied()).and_then(|t| self.wires.get(t));
        match (w, tail) {
            (Some(h), Some(t)) => {
                let outer = |x: &WireRow, first: bool| match (x.from.continuation, x.to.continuation) {
                    (true, false) => x.to.text(style),
                    (false, true) => x.from.text(style),
                    _ if first => x.from.text(style),
                    _ => x.to.text(style),
                };
                (outer(h, true), outer(t, false))
            }
            _ => (w.and_then(|w| w.from.text(style)), w.and_then(|w| w.to.text(style))),
        }
    }

    /// One cable's core list (design 028 §6.3): core, wire number, from, to, state; spare cores
    /// show `spare` in From.
    pub fn core_table(&self, k: CableId) -> Table {
        let mut t = Table { headers: vec!["Core", "Wire number", "From", "To", "State"], rows: Vec::new() };
        let Some(r) = self.cables.get(&k) else { return t };
        let style = self.style;
        let member = |i: usize, label: Option<String>, state: &str| {
            let w = r.members.get(i).and_then(|w| self.wires.get(w));
            let (from, to) = self.chain_ends(r, i, style);
            vec![
                Cell::opt(&label, || "The core has no label.".into()),
                Cell::ok(w.and_then(|w| w.wire_number.clone()).unwrap_or_default()),
                Cell::ok(from.unwrap_or_default()),
                Cell::ok(to.unwrap_or_default()),
                Cell::ok(state),
            ]
        };
        match &r.count.offer {
            Some(offer) => {
                for label in offer {
                    match r.count.matched.iter().position(|m| m.as_deref() == Some(label.as_str())) {
                        Some(i) => t.rows.push(member(i, Some(label.clone()), "used")),
                        None => t.rows.push(vec![Cell::ok(label.clone()), Cell::default(), Cell::ok("spare"), Cell::default(), Cell::ok("spare")]),
                    }
                }
                for i in &r.count.over {
                    t.rows.push(member(*i, r.member_cores.get(*i).cloned().flatten(), "over"));
                }
            }
            None => {
                for i in 0..r.members.len() {
                    t.rows.push(member(i, r.member_cores.get(i).cloned().flatten(), "used"));
                }
            }
        }
        t
    }
}

impl Outputs {
    /// Keep connector `c`'s half rows and their groups (review of #81, 2). `set_device` has taken
    /// the old ones out.
    fn set_connector(&mut self, c: ComponentId, rows: Vec<BomRow>) {
        for r in &rows {
            let (lk, hk) = (loc_key(r.location.as_deref()), half_key(r));
            let n = self.half_loc.entry((lk.clone(), hk.clone())).or_default();
            *n += 1;
            if *n == 1 {
                *self.half_loc_rows.entry(lk).or_default() += 1;
            }
            *self.half_part.entry(hk).or_default() += 1;
        }
        self.connectors.insert(c, rows);
    }

    fn remove_connector(&mut self, c: ComponentId) {
        for r in &self.connectors.remove(&c).unwrap_or_default() {
            let (lk, hk) = (loc_key(r.location.as_deref()), half_key(r));
            if let Some(n) = self.half_loc.get_mut(&(lk.clone(), hk.clone())) {
                *n -= 1;
                if *n == 0 {
                    self.half_loc.remove(&(lk.clone(), hk.clone()));
                    if let Some(m) = self.half_loc_rows.get_mut(&lk) {
                        *m -= 1;
                        if *m == 0 {
                            self.half_loc_rows.remove(&lk);
                        }
                    }
                }
            }
            if let Some(n) = self.half_part.get_mut(&hk) {
                *n -= 1;
                if *n == 0 {
                    self.half_part.remove(&hk);
                }
            }
        }
    }
}
