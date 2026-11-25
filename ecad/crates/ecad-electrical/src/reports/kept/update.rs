//! Keeping the report models up to date: what an edit's effects change, row by row (the update map in [`super`]).

use super::*;

impl ReportModels {
    /// Recompute what `n` names on `doc` (the state after), with text measured in `m`.
    pub(crate) fn update(&mut self, doc: &Document, n: Noted, m: &dyn Look) {
        let Noted { rows, late, plc, strips, numbers, placed, positions } = n;
        self.numbers_moved = numbers;
        let before = self.generated_sheets(doc);
        self.update_rows(doc, Rows { scope: rows, late, plc, placed, positions }, m);
        // Right after the strips, also when no row was in scope.
        self.strip_pages.update(doc, strips, &self.strips);
        if numbers || self.generated_sheets(doc) != before {
            self.refresh_continuation(doc);
            self.revision += 1;
        }
    }

    pub(super) fn update_rows(&mut self, doc: &Document, rows: Rows, m: &dyn Look) {
        let Rows { mut scope, late, plc, placed, positions } = rows;
        // What this update changes, for the generated pages that depend on it.
        self.last_cables_changed = false;
        self.strips.last_changed.clear();
        // The PLC model is built again after every commit (no update map since 0.2.3).
        if self.plc.update_touched(doc, plc, m) {
            self.checks_changed += 1;
            self.revision += 1;
        }
        for p in placed {
            if needs_reports(doc, p) {
                self.generated.insert(p);
            } else {
                self.generated.remove(&p);
            }
        }
        scope.close(doc);
        if scope.is_empty() {
            return;
        }
        let mut work = 0;
        self.style = doc.settings().reference_style();
        self.colour_style = doc.settings().wiring.colour_style();
        if scope.bom_view {
            self.view = BomView::of(doc);
        }
        for k in &scope.potentials {
            scope.wires.extend(self.by_potential.get(k).into_iter().flatten().copied());
        }
        if scope.all_wires {
            scope.wires.extend(self.wires.keys().copied());
        }
        if scope.sheet {
            let own = |p: &PageId| doc.pages().get(p).is_some_and(|x| x.sheet.is_some());
            scope.route_pages.extend(doc.page_order().iter().filter(|p| !own(p)).copied());
        }
        if scope.project_location {
            scope.loose.extend(self.from_project.iter().copied());
            scope.loose.extend(self.from_none.iter().copied());
        }
        scope.wires.extend(late.wires);
        // A connector naming or description default reaches the
        // connectors inheriting it (their half rows); a naming key also their pins' wire rows.
        for k in &scope.connector_keys {
            let naming = crate::defaults::connectors::is_naming_key(k);
            let own = |c: &DeviceId| naming && doc.devices().get(c).is_some_and(|x| x.properties.contains_key(k.as_str()));
            let inheriting: Vec<DeviceId> = self.connectors.keys().filter(|c| !own(c)).copied().collect();
            for c in inheriting {
                scope.devices.insert(c);
                if crate::defaults::connectors::is_naming_key(k) {
                    for (t, _) in doc.connection_points_of(c) {
                        scope.wires.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                    }
                }
            }
        }
        for k in scope.strip_keys.iter().filter(|k| crate::defaults::terminal_blocks::is_label_key(k)) {
            for c in self.strips.inheriting(k) {
                for (t, _) in doc.connection_points_of(c) {
                    scope.wires.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                }
            }
        }
        for id in &scope.loose {
            self.set_loose(doc, *id);
            work += 1;
        }
        let mut cache = BoxCache::default();
        for c in &scope.devices {
            let before = self.devices.get(c).map(|d| (d.tag.clone(), d.location.value.clone()));
            self.set_device(doc, *c, &mut cache);
            work += 1;
            let after = self.devices.get(c).map(|d| (d.tag.clone(), d.location.value.clone()));
            if before != after {
                for (t, _) in doc.connection_points_of(*c) {
                    scope.wires.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                }
            }
        }
        for p in &scope.route_pages {
            if doc.pages().contains_key(p) {
                for r in doc.routes_on(*p).filter_map(|r| doc.routes().get(&r)) {
                    for t in r.points_landed() {
                        scope.wires.extend(doc.ends_at(t).into_iter().map(|(w, _)| w));
                    }
                }
            }
        }
        // Page positions and numbers only when a wire is in scope.
        let mut cx = if scope.wires.is_empty() { None } else { Some(PageCx::of(doc)) };
        for (w, cx) in scope.wires.iter().zip(std::iter::repeat_with(|| cx.as_ref()).flatten()) {
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
        // The strips these rows touch.
        let mut pages = scope.route_pages.clone();
        pages.extend(positions.changed(doc));
        let mut strips = std::mem::take(&mut self.strips);
        let which = strips.touched(
            doc,
            &crate::terminal_strips::strip_model::StripScope {
                devices: &scope.devices,
                wires: &scope.wires,
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
        // A project default reaches only its inheriting strips' accessories.
        if !acc_only.is_empty() {
            strips.recompute_acc(doc, &acc_only);
        }
        // Strip Checks rows read from the strips just recomputed.
        if !strips.last_changed.is_empty() {
            self.checks_changed += 1;
        }
        self.strips = strips;
        self.last_work = work;
        self.revision += 1;
    }

    /// Verify mode: the kept strips against a rebuild on `doc`; a
    /// difference is put right and described (a bug, for the event log).
    pub fn verify_strips_with(&mut self, doc: &Document) -> Option<String> {
        let built = crate::terminal_strips::strip_model::Strips::build(doc, self, &PageCx::of(doc));
        // The graphic pages' kept data is built from the kept strips after every
        // commit (no update map), so it follows them.
        if self.strips.same_as(&built) {
            return None;
        }
        let work = self.strips.last_work;
        self.strips = built;
        self.strips.last_work = work;
        // The page layout follows the rebuilt strips.
        self.strip_pages.adopt(crate::terminal_strips::pages::StripPages::build(doc, &self.strips));
        Some("the terminal strip model was out of step; rebuilt".to_string())
    }

    pub(super) fn remove_device(&mut self, c: DeviceId) {
        self.remove_connector(c);
        let Some(old) = self.devices.remove(&c) else { return };
        self.no_part_number -= usize::from(old.part_number.is_none());
        self.no_location -= usize::from(old.location.value.is_none());
        self.no_function -= usize::from(old.function.key.is_none());
    }

    /// Take every entry of `c` (a device, or an item) out of the rows and counts.
    pub(super) fn remove_entries(&mut self, c: Owner) {
        if let Owner::Loose(id) = c {
            self.from_project.remove(&id);
            self.from_none.remove(&id);
        }
        let keys: Vec<(Owner, u8)> = self.entries.range((c, 0)..=(c, u8::MAX)).map(|(k, _)| *k).collect();
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
            if e.is_item() {
                self.items_no_part -= usize::from(e.part_number.is_none());
            }
            if e.is_item() || e.line > 0 {
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
            if k.0.device().is_some_and(|d| self.for_missing.remove(&(d, k.1))) {
                self.checks_changed += 1;
            }
        }
        let Owner::Device(c) = c else { return };
        if self.bad_part_keys.remove(&c).is_some() {
            self.checks_changed += 1;
        }
        if self.fuse_gaps.remove(&c).is_some() | self.inserts.remove(&c).is_some() {
            self.checks_changed += 1;
        }
    }

    /// The nested layout's row key of main part or item `e`: a device with part lines is a row of
    /// its own (its lines follow it).
    pub(super) fn nested_key(&self, e: &Entry, pk: &PartKey) -> PartKey {
        let has_lines = self.entries.range((e.owner, 1)..=(e.owner, u8::MAX)).next().is_some();
        if has_lines { pk.alone((e.owner, 0)) } else { pk.clone() }
    }

    /// Put entry `e` into the rows and counts.
    pub(super) fn add_entry(&mut self, e: Entry) {
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
        if e.is_item() {
            self.items_no_part += usize::from(e.part_number.is_none());
        }
        if e.is_item() || e.line > 0 {
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
        if let (false, Some(c)) = (e.missing.is_empty(), e.owner.device()) {
            self.for_missing.insert((c, e.line));
            self.checks_changed += 1;
        }
        self.entries.insert((e.owner, e.line), e);
    }

    /// The entries of device `c` (its main part and part lines), and its fuse coverage.
    pub(super) fn set_entries(&mut self, doc: &Document, c: DeviceId, main: Option<Entry>) {
        let Some(comp) = doc.devices().get(&c) else { return };
        let (lines, bad) = bom::lines(&comp.properties);
        if !bad.is_empty() {
            self.bad_part_keys.insert(c, bad);
            self.checks_changed += 1;
        }
        let (tag, location) = match self.devices.get(&c) {
            Some(d) => (d.tag.clone(), d.location.value.clone()),
            None => (String::new(), None),
        };
        // A connector's lines go under a half (`part.<n>.half`, else
        // its first drawn half), with that half's tag and location.
        let halves = crate::connectors::half_rows(doc, c, &mut BoxCache::default());
        // Sub-items only where `for` can name some or fuses can be uncovered.
        let fuseterminalblock = crate::terminal_strips::fuse_terminal_block_of(doc, c).is_some();
        let subs = if fuseterminalblock { bom::Subs::None } else { bom::sub_items(doc, c) };
        let mut covered: BTreeSet<&str> = BTreeSet::new();
        let mut inserts = 0;
        let mut entries = Vec::new();
        for l in &lines {
            let (have, missing): (Vec<String>, Vec<String>) = l.subs.iter().cloned().partition(|s| subs_has(&subs, s));
            covered.extend(l.subs.iter().map(String::as_str));
            inserts += usize::from(l.for_text.is_some() || fuseterminalblock);
            let half = halves.iter().find(|h| Some(h.side) == l.half).or(halves.first());
            entries.push(Entry {
                owner: Owner::Device(c),
                line: l.n,
                tag: half.map_or_else(|| tag.clone(), |h| h.tag.clone()),
                location: half.map_or_else(|| location.clone(), |h| h.location.value.clone()),
                description: l.description.clone(),
                maker: l.maker.clone(),
                part_number: l.part_number.clone(),
                article_number: l.article_number.clone(),
                ratings: Vec::new(),
                qty: l.qty,
                metres: l.metres,
                kit: l.kit.clone(),
                subs: have,
                missing,
            });
        }
        let gaps = if fuseterminalblock {
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

    /// An item's entry and location (its own, else the project's). An
    /// item is a loose part, read through [`ecad_model::objects::loose_part::Bom`].
    pub(super) fn set_loose(&mut self, doc: &Document, id: LoosePartId) {
        use crate::locations::designations::{Aspect, code_of, default_key};
        use ecad_model::objects::loose_part::{Bom, ItemUnit};
        self.remove_entries(Owner::Loose(id));
        let Some(p) = doc.loose_parts().get(&id) else { return };
        // Stored values are entry keys of the project's list; shown by their codes.
        let location = match p.location {
            Some(l) => code_of(doc, Aspect::Location, &l.hex()),
            None => match default_key(doc.project(), Aspect::Location) {
                Some(k) => {
                    self.from_project.insert(id);
                    code_of(doc, Aspect::Location, &k)
                }
                None => {
                    self.from_none.insert(id);
                    None
                }
            },
        };
        for line in p.bom_lines() {
            self.add_entry(Entry {
                owner: Owner::Loose(id),
                line: 0,
                tag: String::new(),
                location: location.clone(),
                description: clean(line.description),
                maker: clean(line.manufacturer),
                part_number: clean(line.part_number),
                article_number: clean(line.article_number),
                ratings: Vec::new(),
                qty: Qty(line.qty.0),
                metres: line.unit == ItemUnit::Metres,
                kit: None,
                subs: Vec::new(),
                missing: Vec::new(),
            });
        }
    }

    pub(super) fn set_device(&mut self, doc: &Document, c: DeviceId, cache: &mut BoxCache) {
        self.remove_device(c);
        self.remove_entries(Owner::Device(c));
        // A jumper bar key on a device that is not a strip.
        let misplaced = crate::terminal_strips::strip_of(doc, c).is_none()
            && doc
                .devices()
                .get(&c)
                .is_some_and(|x| x.properties.keys().any(|k| ecad_model::connectivity::jumper_bars::jumper_bar_number(k).is_some()));
        if misplaced {
            let keys = doc
                .devices()
                .get(&c)
                .map(|x| x.properties.keys().filter(|k| ecad_model::connectivity::jumper_bars::jumper_bar_number(k).is_some()).cloned().collect())
                .unwrap_or_default();
            self.jumper_bar_keys.insert(c, keys);
            self.jumper_bars_off_strip.insert(c);
        } else if self.jumper_bars_off_strip.remove(&c) {
            self.jumper_bar_keys.remove(&c);
            self.checks_changed += 1;
        }
        if misplaced {
            self.checks_changed += 1;
        }
        if !locations::is_device(doc, c) {
            return;
        }
        let Some(comp) = doc.devices().get(&c) else { return };
        let location = locations::resolve_with(doc, c, cache);
        let goto = crate::symbols::reps::goto(doc, c, None)
            .and_then(|rid| doc.representations().get(&rid).map(|r| (rid, r.page, locations::anchor(doc, r))));
        let row = DeviceRow {
            component: c,
            tag: comp.tag_shown().unwrap_or_default(),
            description: clean(comp.description.as_ref().map(|t| t.as_str())),
            maker: clean(comp.manufacturer.as_ref().map(|t| t.as_str())),
            part_number: clean(comp.part_number.as_ref().map(|t| t.as_str())),
            article_number: clean(comp.article_number.as_ref().map(|t| t.as_str())),
            location,
            function: locations::resolve_function(doc, c),
            goto,
        };
        // A connector has one BOM row per drawn half, kept apart;
        // its part lines are entries, each under its half (`part.<n>.half`).
        let connector = crate::connectors::connector_of(doc, c).is_some_and(|x| x.mating);
        if connector {
            let rows = connector_rows(doc, c, cache);
            self.set_connector(c, rows);
        }
        // A strip has one BOM row per terminal type, from the strip model; its
        // part lines are entries like a device's.
        let main = (crate::terminal_strips::strip_of(doc, c).is_none() && !connector).then(|| Entry {
            owner: Owner::Device(c),
            line: 0,
            tag: row.tag.clone(),
            location: row.location.value.clone(),
            description: row.description.clone(),
            maker: row.maker.clone(),
            part_number: row.part_number.clone(),
            article_number: row.article_number.clone(),
            ratings: comp.device_type.rating_parts(),
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

    pub(super) fn end_cell(&self, doc: &Document, t: Option<ConnectionPointId>, at: Option<(PageId, Pt)>, cx: &PageCx) -> EndCell {
        let order = at.map_or((usize::MAX, 0, 0), |(p, a)| {
            let pos = cx.pos(p);
            let place = cx.place(doc, p, a);
            (pos, place.as_ref().map_or(0, |x| x.column), place.as_ref().map_or(0, |x| x.row))
        });
        let Some(t) = t else { return EndCell { order, ..EndCell::default() } };
        let Some(term) = doc.connection_points().get(&t) else { return EndCell { order, ..EndCell::default() } };
        if let Some(a) = term.owner.aid() {
            let name = crate::continuations::display_name(doc, a);
            return EndCell { name: Some(format!("{name} >")), order, continuation: true, ..EndCell::default() };
        }
        let Some(c) = term.device() else { return EndCell { order, ..EndCell::default() } };
        {
            {
                let label = crate::terminal_strips::terminal_blocks::connection_point_name(doc, t);
                let device = self.devices.get(&c);
                let tag = device.map(|d| d.tag.clone()).or_else(|| crate::devices::tags::tag_shown_of(doc, c)).unwrap_or_else(|| "?".into());
                // A connector pin's end is named by its half (`-X10.S:3`).
                let name = crate::connectors::pin_name(doc, t).unwrap_or_else(|| format!("{tag}:{label}"));
                let location = match device {
                    Some(d) => d.location.value.clone(),
                    None => locations::resolve(doc, c).value,
                };
                EndCell { name: Some(name), location, device: Some(c), order, continuation: false }
            }
        }
    }

    /// The cell of conductor end `t` drawn at `at` (a connection point's far end).
    pub(crate) fn end_cell_of(&self, doc: &Document, t: Option<ConnectionPointId>, at: Option<(PageId, Pt)>, cx: &PageCx) -> EndCell {
        self.end_cell(doc, t, at, cx)
    }

    pub(super) fn set_wire(&mut self, doc: &Document, w: WireId, cx: &PageCx) {
        if let Some(old) = self.wires.remove(&w) {
            let k = old.potential.as_deref().unwrap_or("").trim().to_lowercase();
            if let Some(set) = self.by_potential.get_mut(&k) {
                set.remove(&w);
                if set.is_empty() {
                    self.by_potential.remove(&k);
                }
            }
        }
        let Some(cond) = doc.wires().get(&w) else { return };
        // Where its ends are drawn (book order), and the middle of the first segment of the first
        // route landing on its first point.
        let [a_at, b_at] = crate::reports::end_points(doc, w, cx);
        let mid =
            crate::wires::routes::routes_of_wire(doc, w).into_iter().filter_map(|r| doc.routes().get(&r)).min_by_key(|r| cx.pos(r.page)).and_then(
                |r| {
                    let (Some(a), Some(b)) = (r.points.first(), r.points.get(1).or(r.points.first())) else { return None };
                    Some((r.page, Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)))
                },
            );
        let mut ends = [self.end_cell(doc, Some(cond.end(End::A)), a_at, cx), self.end_cell(doc, Some(cond.end(End::B)), b_at, cx)];
        if ends[1].order < ends[0].order {
            ends.swap(0, 1);
        }
        let [from, to] = ends;
        let conn = doc.connectivity();
        let net = conn.net_of_conductor(w).and_then(|id| conn.net(id));
        let pot = net.map(|n| ecad_model::connectivity::wiring::net_potential(doc, n));
        // The wire's category gives what the wire has none of its own.
        let draw = cx.draws.draw(doc, w);
        let cross_section = draw.cross_section.as_ref().map(|v| v.0.clone());
        let colour = draw.colour.as_ref().map(|v| v.0.to_string());
        let colour_own = draw.colour.as_ref().is_some_and(|v| v.1 == ecad_model::registry::Source::Object);
        let cross_section_own = draw.cross_section.as_ref().is_some_and(|v| v.1 == ecad_model::registry::Source::Object);
        let category = draw.category.clone().map(|id| {
            let n = ecad_model::registry::styles::name(doc, &crate::styles::wire::CATALOGUE, &id);
            (id, n)
        });
        let part = draw.category.as_ref().and_then(|id| {
            let f = |k: &str| {
                ecad_model::registry::styles::field_of(doc, &crate::styles::wire::CATALOGUE, id, k, None)
                    .and_then(|r| crate::styles::wire::filled(&r))
            };
            let pn = f("part_number")?;
            let allowance =
                ecad_model::registry::styles::field_of(doc, &crate::styles::wire::CATALOGUE, id, "allowance", None).map_or(0.0, |r| r.number());
            Some((pn, f("maker"), f("description"), allowance))
        });
        let cable = doc
            .cables_of(w)
            .into_iter()
            .filter_map(|k| {
                let cab = doc.cables().get(&k)?;
                let tag = cab.tag_shown().unwrap_or_else(|| "?".into());
                let i = cab.cores.iter().position(|m| m.wire() == Some(w))?;
                let core = cab.cores.get(i).and_then(|m| m.label_str().map(str::to_string)).unwrap_or_else(|| (i + 1).to_string());
                Some(format!("{tag} core {core}"))
            })
            .collect::<Vec<_>>()
            .join(", ");
        // A wire between two terminals of one strip is a bridge.
        let cable = match crate::terminal_strips::bridge_of(doc, w) {
            Some(strip) if cable.is_empty() => format!("Bridge {}", crate::devices::tags::tag_shown_of(doc, strip).unwrap_or_else(|| "?".into())),
            _ => cable,
        };
        let page = mid.and_then(|(p, a)| cx.place(doc, p, a));
        let potential = pot.and_then(|p| p.name);
        self.by_potential.entry(potential.as_deref().unwrap_or("").trim().to_lowercase()).or_default().insert(w);
        self.wires.insert(
            w,
            WireRow {
                conductor: w,
                wire_number: ecad_model::connectivity::wiring::wire_number(doc, w),
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

    pub(super) fn set_cable(&mut self, doc: &Document, k: CableId) {
        let before = self.cables.get(&k).cloned();
        self.set_cable_row(doc, k);
        if self.cables.get(&k) != before.as_ref() {
            self.last_cables_changed = true;
            let rows = |r: Option<&CableRow>| -> [usize; 3] {
                r.map_or([0, 0, 0], |r| {
                    let known = r.count.offer.is_some();
                    // A cable not drawn has no core-count row.
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

    pub(super) fn set_cable_row(&mut self, doc: &Document, k: CableId) {
        let Some(cab) = doc.cables().get(&k) else {
            self.cables.remove(&k);
            return;
        };
        let p = |key: &str| clean(cab.get(key));
        // Cores are counted by chain; each chain is shown by its first member,
        // with the first stored label along it.
        let (heads, tails) = chain_heads(doc, k, cab);
        let mut locs: BTreeSet<(LocKey, String)> = BTreeSet::new();
        let mut missing = false;
        // The devices at the cores' ends, per location.
        let mut at_loc: BTreeMap<String, BTreeSet<((String, u64, String), DeviceId)>> = BTreeMap::new();
        for m in &cab.cores {
            let Some(cond) = m.wire().and_then(|w| doc.wires().get(&w)) else { continue };
            for t in &cond.ends {
                let Some(c) = doc.connection_points().get(t).and_then(|x| x.device()) else { continue };
                let (loc, tag) = match self.devices.get(&c) {
                    Some(d) => (d.location.value.clone(), d.tag.clone()),
                    None => (locations::resolve(doc, c).value, crate::devices::tags::tag_shown_of(doc, c).unwrap_or_else(|| "?".into())),
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
            let at = r.items.first().and_then(|i| doc.items().get(i)).and_then(|i| match &i.kind {
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
                // The chains' heads, then the stored cores with no wire (spare or on a loose line).
                count: crate::terminal_strips::cores::count_cores(
                    crate::styles::cable::offer(doc, k),
                    &heads.iter().cloned().chain(cab.cores.iter().filter(|m| m.wire().is_none()).cloned()).collect::<Vec<_>>(),
                ),
                members: heads.iter().filter_map(|m| m.wire()).collect(),
                member_cores: heads.iter().map(|m| m.label_str().map(str::to_string)).collect(),
                tails,
                from_devices,
                to_devices,
                drawn: crate::cables::is_drawn(doc, k),
                planned: crate::cables::Side::BOTH.map(|side| crate::cables::plan_words(doc, k, side)),
                glands: crate::cables::Side::BOTH.map(|side| crate::cables::gland(doc, k, side).map(|(g, _)| g)),
                shield: crate::cables::shield(doc, k)
                    .map(|(w, _)| w)
                    .filter(|w| *w != ecad_model::objects::styles::cable::Shield::None)
                    .map(|w| ecad_model::naming::words::Word::word(w).to_string()),
            },
        );
    }
}

impl ReportModels {
    /// Keep connector `c`'s half rows and their groups. `set_device` has taken
    /// the old ones out.
    pub(super) fn set_connector(&mut self, c: DeviceId, rows: Vec<BomRow>) {
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

    pub(super) fn remove_connector(&mut self, c: DeviceId) {
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
