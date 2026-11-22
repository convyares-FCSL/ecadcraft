//! The kept results and their update map (design 057 §10): what an op reaches ([`Touch`]), and the results and indexes [`Kept`] re-evaluates.

use super::*;

/// The count kinds' tallies, kept by [`Kept`] without a rescan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    pub bundling_unset: usize,
    pub no_device: usize,
    pub no_data: usize,
    pub assumed: usize,
    pub no_current: usize,
}

/// What an op reaches (collected on the state before and after, as `checks::Scope` does).
#[derive(Debug, Clone, Default)]
pub struct Touch {
    pub subjects: BTreeSet<RatedSubject>,
    /// Re-trace these too.
    pub retrace: BTreeSet<RatedSubject>,
    pub nets: BTreeSet<NetKey>,
    pub devices: BTreeSet<DeviceId>,
    pub styles: BTreeSet<(ecad_model::StyleCatalogue, StyleRef)>,
    /// Locations whose temperature changed: `at[location]` (D1).
    pub locations: BTreeSet<String>,
    /// Location boxes to read again.
    pub boxes: BTreeSet<ecad_model::AidId>,
    /// Devices whose location may have changed: their wires' results.
    pub moved: BTreeSet<DeviceId>,
    /// Pages a location box moved on: the wires of the devices drawn there.
    pub pages: BTreeSet<ecad_model::PageId>,
    /// Every result again (no re-trace).
    pub all_results: bool,
    /// The rating table's own rows (bad rows, odd pairs) again.
    pub table: bool,
}

impl Touch {
    /// Add what `ops` reach in `doc`: [`Touch::note`] for each of their effects.
    pub fn collect(&mut self, doc: &Document, ops: &[Op]) {
        for effect in &ecad_model::document::effect::Effects::of(ops) {
            self.note(doc, effect);
        }
    }

    /// Add what one effect reaches, looked up in `doc` (call once on the state before, once after).
    pub fn note(&mut self, doc: &Document, effect: &ecad_model::document::effect::Effect<'_>) {
        use ecad_model::document::effect::Effect;
        let conn = doc.connectivity();
        let net_of_connection_point = |t: ecad_model::ConnectionPointId, s: &mut Touch| {
            if let Some(n) = conn.net_of(&ecad_model::Node::ConnectionPoint(t)) {
                s.nets.insert(n);
            }
        };
        match effect {
            Effect::Prop { object: ObjectRef::Wire(c), .. } => {
                self.retrace.insert(RatedSubject::Wire(*c));
                for k in doc.cables_of(*c) {
                    self.retrace.insert(RatedSubject::Cable(k));
                }
            }
            Effect::Wire { id: c, before: Some(before), after: Some(after) } => {
                self.retrace.insert(RatedSubject::Wire(*c));
                if (before.potential != after.potential || before.number != after.number)
                    && let Some(n) = conn.net_of_conductor(*c)
                {
                    self.nets.insert(n);
                }
                for k in doc.cables_of(*c) {
                    self.retrace.insert(RatedSubject::Cable(k));
                }
            }
            // Made or deleted.
            Effect::Wire { id, before, after } => {
                self.retrace.insert(RatedSubject::Wire(*id));
                if let Some(n) = conn.net_of_conductor(*id) {
                    self.nets.insert(n);
                }
                for t in before.iter().chain(after.iter()).flat_map(|w| w.ends.iter()) {
                    net_of_connection_point(*t, self);
                }
            }
            Effect::Prop { object: ObjectRef::Device(c), key, .. } => {
                let key = key.raw;
                if crate::symbols::rules::is_rating_key(key) || key == ecad_model::naming::tag::TAG_KEY {
                    self.devices.insert(*c);
                } else if ecad_model::naming::keys::area_of(key) == ecad_model::naming::keys::Area::JumperBar {
                    for (t, _) in doc.connection_points_of(*c) {
                        net_of_connection_point(t, self);
                    }
                }
            }
            Effect::Prop { object: ObjectRef::Project | ObjectRef::Standard, key, .. } => {
                if matches!(key.area, ecad_model::naming::keys::Area::Rating | ecad_model::naming::keys::Area::Wire) {
                    self.all_results = true;
                    self.table |= key.raw == defaults::rating::TABLE.key;
                }
            }
            // A page's properties are read by no rating row; a cable is written whole.
            Effect::Prop { object: ObjectRef::Page(_) | ObjectRef::Cable(_), .. } => {}
            // Brief 052: a location entry's code changed (or the entry came or went): the rows
            // naming it are worded again.
            Effect::Project { before, after } => {
                self.locations.extend(crate::locations::designations::changed_codes(before, after, crate::locations::designations::Aspect::Location));
            }
            Effect::Style { catalogue, id, .. } => {
                if *catalogue == ecad_model::StyleCatalogue::RatingTable {
                    self.all_results = true;
                    self.table = true;
                } else if *catalogue == wire::CATALOGUE.catalogue || *catalogue == ecad_model::StyleCatalogue::Cable {
                    self.styles.insert((*catalogue, (*id).clone()));
                    // A category naming a potential reaches every wire of it.
                    if *catalogue == wire::CATALOGUE.catalogue {
                        self.all_results = true;
                    }
                }
            }
            Effect::Standard { .. } => {
                self.all_results = true;
                self.table = true;
            }
            Effect::Settings { before, after } => {
                if before.wiring.potentials != after.wiring.potentials {
                    self.all_results = true;
                }
            }
            Effect::ConnectionPoint { id, .. } => net_of_connection_point(*id, self),
            // A location box made or deleted.
            Effect::Aid { id, before: None, after: Some(aid) } | Effect::Aid { id, before: Some(aid), after: None } => {
                if let Some(b) = aid.as_box() {
                    self.boxes.insert(*id);
                    self.locations.extend(crate::locations::designations::box_key(b, crate::locations::designations::Aspect::Location));
                    self.box_pages(doc, *id);
                }
            }
            // Design 072 §6.2: a box's temperature reaches the wires at its location; its new
            // location the wires at both locations and those of the devices on its pages.
            Effect::Aid { id, before, after } => {
                let loc = |a: &ecad_model::Aid| {
                    a.as_box().and_then(|b| crate::locations::designations::box_key(b, crate::locations::designations::Aspect::Location))
                };
                if before.iter().chain(after.iter()).any(|a| a.as_box().is_some()) {
                    self.boxes.insert(*id);
                    let (b, a) = (before.and_then(loc), after.and_then(loc));
                    if b != a {
                        self.box_pages(doc, *id);
                    }
                    self.locations.extend([b, a].into_iter().flatten());
                }
            }
            // Lane 072 B: a device's description, part or type text is read by no rating row.
            Effect::Device { before: Some(before), after: Some(after), .. } if before.only_text_differs(after) => {}
            // Design 072 §2.2: a device's rating reaches only the subjects it protects.
            Effect::Device { id, before: Some(before), after: Some(after) } if before.only_rating_differs(after) => {
                self.devices.insert(*id);
            }
            // Design 072 §6.3: a box's new location reaches the wires at both locations; a
            // device's own location its wires only.
            Effect::Device { id, before: Some(before), after: Some(after) } if before.only_place_differs(after) => {
                if before.location != after.location {
                    self.moved.insert(*id);
                }
            }
            Effect::Device { id, .. } | Effect::DeviceDefinition { id, .. } => {
                self.devices.insert(*id);
                for (t, _) in doc.connection_points_of(*id) {
                    net_of_connection_point(t, self);
                }
            }
            Effect::Definition { id, .. } => {
                for c in doc.components_using(*id) {
                    for (t, _) in doc.connection_points_of(c) {
                        net_of_connection_point(t, self);
                    }
                }
            }
            Effect::Cable { id, before, after } => {
                self.retrace.insert(RatedSubject::Cable(*id));
                let cores = before.iter().chain(after.iter()).flat_map(|c| c.cores.iter());
                self.retrace.extend(cores.filter_map(|m| m.wire()).map(RatedSubject::Wire));
            }
            // A device moved may enter another location box: its wires; a box moved: the wires at
            // its location and those of the devices on its page. The representation as it is
            // after (as it was, when unlinked).
            Effect::Rep { before, after, .. } => {
                if let Some(rep) = after.or(*before) {
                    match rep.subject {
                        ecad_model::Subject::Aid(a) if crate::locations::box_values(doc, a).is_some() => {
                            self.boxes.insert(a);
                            self.box_locations(doc, a);
                            self.pages.insert(rep.page);
                        }
                        ecad_model::Subject::Device(c) => {
                            self.moved.insert(c);
                        }
                        _ => {}
                    }
                }
            }
            Effect::Page { .. }
            | Effect::Layer { .. }
            | Effect::Item { .. }
            | Effect::Route { .. }
            | Effect::Version { .. }
            | Effect::Folder { .. }
            | Effect::LoosePart { .. }
            | Effect::DeviceLink { .. } => {}
        }
    }
}

/// The kept results and indexes (design 057 §10).
#[derive(Debug, Clone, Default)]
pub struct Kept {
    pub results: BTreeMap<RatedSubject, Rating>,
    traces: BTreeMap<RatedSubject, Found>,
    /// device → subjects it protects (by the trace or by name).
    protects: BTreeMap<DeviceId, BTreeSet<RatedSubject>>,
    /// net → subjects whose trace crossed it.
    path: BTreeMap<NetKey, BTreeSet<RatedSubject>>,
    tally: Tally,
    /// Devices whose rating does not read → how many results name them.
    unread: BTreeMap<DeviceId, (usize, String)>,
    /// The rating table's own rows.
    table_rows: Vec<Issue>,
    /// How many subjects the last build or update evaluated, and re-traced (the update map's
    /// counters, design 057 §11 check 16).
    pub evaluated: usize,
    pub retraced: usize,
    /// Bumped by every build and update; with [`Kept::last`], a view that copied the results
    /// at epoch `n` patches only `last` at `n + 1` (design 057 §10: tables patch the re-checked
    /// rows only).
    pub epoch: u64,
    /// The subjects the last update re-checked.
    pub last: BTreeSet<RatedSubject>,
    /// The methods the chosen rating table has capacity rows for (the Method drop-down).
    pub methods: Vec<&'static str>,
    /// The chosen rating table as last parsed; read again only when an op reaches it.
    table: Option<std::sync::Arc<ratingtable::Table>>,
    /// Location boxes that set a temperature (read again only when an op reaches one).
    boxes: BTreeMap<ecad_model::AidId, BoxTemp>,
    /// location → the wires whose ends are there (design 057 §10, `at`).
    at: BTreeMap<String, BTreeSet<RatedSubject>>,
    /// Panel wires a device protects that have no rating data (`rating-no-data`, §7 and check 13),
    /// kept while the project rates anything.
    unrated: BTreeSet<WireId>,
}

impl Kept {
    /// Every subject, from scratch (open, reload, first show).
    pub fn build(doc: &Document) -> Self {
        let mut k = Kept { boxes: box_temps(doc), ..Kept::default() };
        let mut ctx = Context::with_boxes(doc, &k.boxes);
        ctx.table = ratingtable::chosen_table(doc).map(std::sync::Arc::new);
        k.table = ctx.table.clone();
        for s in subjects(doc, &ctx) {
            k.put(doc, &mut ctx, s, true);
        }
        k.unrated_pass(doc, &mut ctx);
        k.table_rows = table_issues(&ctx);
        k.methods = ctx.table.as_ref().map(|t| t.data.methods()).unwrap_or_default();
        k.epoch = 1;
        k
    }

    pub(super) fn untally(&mut self, r: &Rating) {
        let t = &mut self.tally;
        for i in &r.issues {
            match i {
                Issue::BundlingUnset => t.bundling_unset = t.bundling_unset.saturating_sub(1),
                Issue::NoDevice(false) => t.no_device = t.no_device.saturating_sub(1),
                Issue::NoData(_) => t.no_data = t.no_data.saturating_sub(1),
                Issue::Assumed => t.assumed = t.assumed.saturating_sub(1),
                Issue::NoCurrent => t.no_current = t.no_current.saturating_sub(1),
                Issue::DeviceUnread(c, _) => {
                    if let Some(e) = self.unread.get_mut(c) {
                        e.0 = e.0.saturating_sub(1);
                        if e.0 == 0 {
                            self.unread.remove(c);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn tally(&mut self, r: &Rating) {
        let t = &mut self.tally;
        for i in &r.issues {
            match i {
                Issue::BundlingUnset => t.bundling_unset += 1,
                Issue::NoDevice(false) => t.no_device += 1,
                Issue::NoData(_) => t.no_data += 1,
                Issue::Assumed => t.assumed += 1,
                Issue::NoCurrent => t.no_current += 1,
                Issue::DeviceUnread(c, text) => self.unread.entry(*c).or_insert((0, text.clone())).0 += 1,
                _ => {}
            }
        }
    }

    pub(super) fn forget(&mut self, s: RatedSubject) {
        if let RatedSubject::Wire(c) = s {
            self.unrated.remove(&c);
        }
        if let Some(old) = self.results.remove(&s) {
            self.untally(&old);
            for l in &old.locations {
                if let Some(set) = self.at.get_mut(l) {
                    set.remove(&s);
                    if set.is_empty() {
                        self.at.remove(l);
                    }
                }
            }
            if let Some(d) = &old.device
                && let Some(set) = self.protects.get_mut(&d.component)
            {
                set.remove(&s);
            }
        }
        if let Some(f) = self.traces.remove(&s) {
            for n in &f.path {
                if let Some(set) = self.path.get_mut(n) {
                    set.remove(&s);
                    if set.is_empty() {
                        self.path.remove(n);
                    }
                }
            }
            for (c, _) in &f.devices {
                if let Some(set) = self.protects.get_mut(c) {
                    set.remove(&s);
                }
            }
        }
    }

    /// Evaluate `s` again (re-tracing it when `retrace`), or drop it when it is no longer rated.
    pub(super) fn put(&mut self, doc: &Document, ctx: &mut Context, s: RatedSubject, retrace: bool) {
        let rated = match s {
            RatedSubject::Wire(c) => wire_is_rated(doc, ctx, c),
            RatedSubject::Cable(k) => cable_is_rated(doc, ctx, k),
        };
        let found = if retrace || !self.traces.contains_key(&s) { None } else { self.traces.get(&s).cloned() };
        self.forget(s);
        if !rated {
            // A protected panel wire with no rating data, counted while the project rates
            // anything (§7, check 13).
            if let RatedSubject::Wire(c) = s
                && !self.results.is_empty()
                && is_panel_wire(doc, c)
                && doc.wires.contains_key(&c)
            {
                let f = found.unwrap_or_else(|| {
                    self.retraced += 1;
                    trace::trace_subject(doc, s)
                });
                if !f.devices.is_empty() {
                    self.unrated.insert(c);
                    // On the path index (a connectivity change re-traces it), not on `protects`: a
                    // device's rating does not change whether it has rating data.
                    for n in &f.path {
                        self.path.entry(*n).or_default().insert(s);
                    }
                    self.traces.insert(s, f);
                }
            }
            return;
        }
        self.evaluated += 1;
        let found = match found {
            Some(f) => f,
            None => {
                self.retraced += 1;
                trace::trace_subject(doc, s)
            }
        };
        let r = evaluate(doc, ctx, s, &found);
        for n in &found.path {
            self.path.entry(*n).or_default().insert(s);
        }
        for (c, _) in &found.devices {
            self.protects.entry(*c).or_default().insert(s);
        }
        if let Some(d) = &r.device {
            self.protects.entry(d.component).or_default().insert(s);
        }
        self.tally(&r);
        for l in &r.locations {
            self.at.entry(l.clone()).or_default().insert(s);
        }
        self.traces.insert(s, found);
        self.results.insert(s, r);
    }

    /// The protected panel wires with no rating data, from scratch: while the project rates
    /// anything, one pass over the panel wires (open, and the rare ops that change what is rated);
    /// none otherwise.
    pub(super) fn unrated_pass(&mut self, doc: &Document, ctx: &mut Context) {
        if self.results.is_empty() {
            for c in std::mem::take(&mut self.unrated) {
                self.forget(RatedSubject::Wire(c));
            }
            return;
        }
        let wires: Vec<WireId> = doc.wires.keys().copied().filter(|c| !self.results.contains_key(&RatedSubject::Wire(*c))).collect();
        for c in wires {
            self.put(doc, ctx, RatedSubject::Wire(c), false);
        }
    }

    /// How many protected wires have no rating data.
    pub fn unrated(&self) -> usize {
        self.unrated.len()
    }

    /// Re-check what `touch` reaches on `doc` (the state after the ops). Returns the subjects
    /// whose rows may have changed.
    pub fn update(&mut self, doc: &Document, touch: &Touch) -> BTreeSet<RatedSubject> {
        self.evaluated = 0;
        self.retraced = 0;
        // The rating table is parsed again only when an op reaches it (FP8).
        let fresh = touch.table || touch.all_results;
        // Location boxes reached are read again (D1); the rest are kept.
        for c in &touch.boxes {
            match box_temp(doc, *c) {
                Some(b) => self.boxes.insert(*c, b),
                None => self.boxes.remove(c),
            };
        }
        let mut ctx = Context::with_boxes(doc, &self.boxes);
        ctx.table = if fresh { ratingtable::chosen_table(doc).map(std::sync::Arc::new) } else { self.table.clone() };
        if fresh {
            self.table = ctx.table.clone();
        }
        let mut retrace: BTreeSet<RatedSubject> = touch.retrace.clone();
        for n in &touch.nets {
            if let Some(set) = self.path.get(n) {
                retrace.extend(set.iter().copied());
            }
        }
        // New subjects on touched nets: the conductors there and their cables.
        let conn = doc.connectivity();
        for n in &touch.nets {
            if let Some(net) = conn.net(*n) {
                for c in &net.wires {
                    retrace.insert(RatedSubject::Wire(*c));
                    for k in doc.cables_of(*c) {
                        retrace.insert(RatedSubject::Cable(k));
                    }
                }
            }
        }
        let mut results: BTreeSet<RatedSubject> = BTreeSet::new();
        for c in &touch.devices {
            if let Some(set) = self.protects.get(c) {
                results.extend(set.iter().copied());
            }
        }
        // Subjects that name a device by hand.
        if !touch.devices.is_empty() {
            results.extend(self.results.iter().filter(|(_, r)| r.device.as_ref().is_some_and(|d| d.how == How::Named)).map(|(s, _)| *s));
        }
        for (cat, id) in &touch.styles {
            if *cat == ecad_model::StyleCatalogue::Cable {
                results.extend(cable::users(doc, id).into_iter().map(RatedSubject::Cable));
            }
        }
        // D1: a location's temperature reaches the wires at it; a device moved, its wires; a box
        // moved, the wires of the devices on its page.
        for l in &touch.locations {
            if let Some(set) = self.at.get(l) {
                results.extend(set.iter().copied());
            }
        }
        let mut moved: BTreeSet<DeviceId> = touch.moved.clone();
        for page in &touch.pages {
            moved.extend(doc.representations_on(*page).filter_map(|(_, r)| match r.subject {
                ecad_model::Subject::Device(c) => Some(c),
                _ => None,
            }));
        }
        for c in &moved {
            for (t, _) in doc.connection_points_of(*c) {
                results.extend(doc.ends_at(t).into_iter().map(|(w, _)| RatedSubject::Wire(w)));
            }
        }
        if touch.all_results || touch.table {
            results.extend(self.results.keys().copied());
            // Subjects that become rated by the change (a category gaining a rated current, a
            // table chosen): one pass over the wires and cables, on these rare ops only.
            results.extend(subjects(doc, &ctx));
        }
        let mut changed = BTreeSet::new();
        for s in &retrace {
            self.put(doc, &mut ctx, *s, true);
            changed.insert(*s);
        }
        for s in results.difference(&retrace) {
            self.put(doc, &mut ctx, *s, false);
            changed.insert(*s);
        }
        if touch.all_results || touch.table {
            self.unrated_pass(doc, &mut ctx);
        }
        if touch.table || touch.all_results {
            self.table_rows = table_issues(&ctx);
            self.methods = ctx.table.as_ref().map(|t| t.data.methods()).unwrap_or_default();
        }
        self.epoch += 1;
        self.last = changed.clone();
        changed
    }

    pub fn tally_now(&self) -> &Tally {
        &self.tally
    }

    /// Location boxes whose temperature does not read: (box, location, text).
    pub fn bad_boxes(&self) -> impl Iterator<Item = (ecad_model::AidId, &str, &str)> {
        self.boxes.iter().filter_map(|(c, b)| match &b.temp {
            Temp::Unread(v) => Some((*c, b.location.as_str(), v.as_str())),
            Temp::Read(_) => None,
        })
    }

    /// Devices whose rating does not read, with their rating text.
    pub fn unread(&self) -> impl Iterator<Item = (DeviceId, &str)> {
        self.unread.iter().map(|(c, (_, t))| (*c, t.as_str()))
    }

    pub fn table_rows(&self) -> &[Issue] {
        &self.table_rows
    }

    /// The subjects a device protects.
    pub fn protected_by(&self, c: DeviceId) -> usize {
        self.protects.get(&c).map_or(0, BTreeSet::len)
    }

    /// Whether any subject uses the Table basis.
    pub fn any_table(&self) -> bool {
        self.results.values().any(|r| r.basis == Basis::Table)
    }
}

/// The rating table's own rows: bad rows, odd pairs (§5.2, §5.4).
pub(super) fn table_issues(ctx: &Context) -> Vec<Issue> {
    let Some(t) = &ctx.table else { return Vec::new() };
    let mut v = Vec::new();
    if let Some(why) = &t.data.too_big {
        v.push(Issue::NoTable(format!("Rating table '{}': {why}", t.name)));
    }
    if t.data.bad_count > 0 {
        let first = t.data.bad.first().map(|(n, why)| format!("line {n}, {why}")).unwrap_or_default();
        v.push(Issue::BadValue(
            "rows",
            format!("Rating table '{}': {} row{} not read (first: {first})", t.name, t.data.bad_count, if t.data.bad_count == 1 { "" } else { "s" }),
        ));
    }
    if let Some(odd) = t.data.odd() {
        v.push(Issue::NoData(format!("Rating table '{}': {odd}", t.name)));
    }
    v
}

impl Touch {
    pub(super) fn box_locations(&mut self, doc: &Document, a: ecad_model::AidId) {
        self.locations.extend(
            crate::locations::box_values(doc, a)
                .and_then(|x| crate::locations::designations::box_key(x, crate::locations::designations::Aspect::Location)),
        );
    }

    pub(super) fn box_pages(&mut self, doc: &Document, a: ecad_model::AidId) {
        self.pages.extend(doc.representations_of(ecad_model::Subject::Aid(a)).map(|(_, r)| r.page));
    }

    /// Whether the ops reach anything here.
    pub fn any(&self) -> bool {
        !(self.subjects.is_empty()
            && self.retrace.is_empty()
            && self.nets.is_empty()
            && self.devices.is_empty()
            && self.styles.is_empty()
            && self.locations.is_empty()
            && self.boxes.is_empty()
            && self.moved.is_empty()
            && self.pages.is_empty())
            || self.all_results
            || self.table
    }
}
