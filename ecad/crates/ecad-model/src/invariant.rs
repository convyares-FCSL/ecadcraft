//! The document's invariants: the full check and the check scoped to what a transaction touched.
//!
//! [`Document::check`] verifies every invariant over the whole document. [`Document::check_touched`]
//! verifies the same predicates, but only for the objects a transaction created, modified or
//! removed ([`Touched`]) and the objects whose invariants depend on them, found through the
//! reverse indexes (the private `index` module). Its cost follows the size of the change, not of the project.
//!
//! # Why the scoped check is enough
//! Every invariant below relates a fixed, small set of objects. A state that was valid before a
//! transaction can only become invalid where the transaction changed one of those objects. So
//! the scoped check re-checks every invariant instance that involves a touched object: the
//! touched object's own invariants, and those of every object that refers to it (looked up in
//! the end state of the transaction, where any dangling reference would be).
//!
//! | # | Invariant | Objects it relates | Re-checked for |
//! |---|-----------|--------------------|----------------|
//! | I1 | The page order lists each page exactly once | pages, page order | any page added or removed (whole order, which is short) |
//! | I2 | An item's page exists (DOC-02, DOC-07) | item → page | touched items; items on a touched page |
//! | I3 | An item's layer exists (DOC-02, DOC-07) | item → layer | touched items; items on a touched layer |
//! | I4 | A primitive item's geometry is finite | item | touched items |
//! | I5 | A symbol item's definition exists (DOC-07) | item → definition | touched items; items placing a touched definition |
//! | I6 | A symbol item's placement is valid | item | touched items |
//! | I7 | A definition's geometry is finite, its terminal keys are unique, and each bridge key is one of them or a placement key (`.s`, `.p`, a level key; brief 044); its text slots are finite, for known device texts, one per text; its defaults use the known keys; it is not derived from its own origin (brief 048) | definition | touched definitions |
//! | I8 | A terminal's component exists (DOC-07) | terminal → component | touched terminals; terminals of a touched component |
//! | I9 | A component's definition exists (DOC-07) | component → definition | touched components; components using a touched definition |
//! | I10 | A representation's subject exists (DOC-07) | representation → component, conductor or cable | touched representations; representations of a touched subject |
//! | I11 | A representation's page exists | representation → page | touched representations; representations on a touched page |
//! | I12 | A representation's items exist and lie on its page | representation → items | touched representations; representations listing a touched item |
//! | I13 | An item is listed once, by at most one representation (DOC-05) | representation ↔ representation, through an item | touched representations (all listings of each of their items are counted); representations listing a touched item |
//! | I14 | A symbol representation holds exactly one symbol instance and depicts a component; without connection points it shows that component's own definition (with them, any embedded definition: decision 2026-11-08, brief 018) | representation → item, representation → component → definition | touched representations; representations listing a touched item; representations of a touched component |
//! | I15 | A drawn representation holds no symbol instance | representation → items | touched representations; representations listing a touched item |
//! | I16 | Connection points belong to a component representation, name that component's terminals and are finite | representation → terminals → component | touched representations; representations placing a touched terminal; representations of a touched component |
//! | I17 | Wire ends attach to existing terminals (DOC-03, DOC-07) | conductor → terminal | touched conductors; conductors attached to a touched terminal |
//! | I18 | Cable members exist and are listed once (DOC-04, DOC-07) | cable → conductors | touched cables; cables listing a touched conductor |
//! | I19 | The folder order lists each folder exactly once (brief 019) | folders, folder order | any folder added or removed (whole order, which is short) |
//! | I20 | A folder's parent exists, the parents form no cycle and it nests at most 8 deep | folder → parent folders | touched folders, their sub-folders (all levels) and the sub-folders of a removed one |
//! | I21 | A page's `folder` property names an existing folder | page → folder | touched pages and pages whose properties changed; pages in a touched folder |
//! | I22 | The location and function lists hold each id and code once, codes with their sign | project | any change to the project's own data |
//! | I23 | A route has 2 to 4096 finite points, lies on an existing page, and its ends land on existing connection points and on other routes of its page (design 072 F31) | route → page, terminals, routes | touched routes; routes on a touched page, landing on a touched terminal (a cable's or a cable reference's named points count as touched with it) or on a touched route |
//! | I25 | A drawing aid is well formed: a box's panel temperature is a number (design 072 §6.2) | aid | touched aids |
//!
//! Not invariants of the document: properties (other than a page's `folder`), layer names, project
//! settings. Named versions are
//! checked by the ledger against the transaction's position (it needs the log, not the document).
//!
//! # Safety net
//! The ledger also runs the full check, and compares the kept index with a rebuilt one, after
//! every transaction with the `verify-invariants` cargo feature (on for every `cargo test`
//! through dev-dependencies), when `ECAD_VERIFY_INVARIANTS=1`, and in the debugger's Verify replay,, and treats
//! any disagreement as a bug ([`crate::ledger`]).
//!
//! This module changes nothing: it only reads the document.

use std::collections::{BTreeMap, BTreeSet};

use crate::doc::{Cable, Definition, Device, Document, Form, Item, ItemKind, Primitive, Representation, Subject, Terminal, Wire};
use crate::effect::{Effect, Effects};
use crate::error::{ModelError, Result};
use crate::id::{CableId, DefinitionId, DeviceId, FolderId, ItemId, LayerId, PageId, RepresentationId, RouteId, TerminalId, WireId};
use crate::index::referrers;
use crate::op::{ObjectRef, Op};
use crate::point::PointKey;

/// What a transaction created, modified or removed: the input of [`Document::check_touched`].
///
/// # Semantics
/// Only the objects an op names directly. The objects that depend on them are found by
/// [`Document::check_touched`] through the reverse indexes, in the state being checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Touched {
    /// A page was added or removed, so the page order changed.
    pub page_order: bool,
    pub pages: BTreeSet<PageId>,
    pub layers: BTreeSet<LayerId>,
    pub items: BTreeSet<ItemId>,
    pub devices: BTreeSet<DeviceId>,
    pub terminals: BTreeSet<TerminalId>,
    pub representations: BTreeSet<RepresentationId>,
    pub wires: BTreeSet<WireId>,
    pub cables: BTreeSet<CableId>,
    /// Routes drawn, removed or changed (design 072 F31).
    pub routes: BTreeSet<RouteId>,
    /// Drawing aids made, removed or changed (design 072 §6.2).
    pub aids: BTreeSet<crate::id::AidId>,
    pub definitions: BTreeSet<DefinitionId>,
    /// A folder was added or removed, so the folder order changed.
    pub folder_order: bool,
    pub folders: BTreeSet<FolderId>,
    /// Pages whose properties changed (only I21 depends on them).
    pub page_properties: BTreeSet<PageId>,
    /// The standard or a style record changed (brief 050a): the style invariants are checked.
    pub styles: bool,
    /// The project's own data changed (design 072 §5.3): I22 is checked.
    pub project: bool,
}

impl Touched {
    /// What `ops` touch, together.
    pub fn of(ops: &[Op]) -> Self {
        let mut t = Touched::default();
        for e in &Effects::of(ops) {
            t.add_effect(e);
        }
        t
    }

    /// Add what `op` touches.
    pub fn add(&mut self, op: &Op) {
        for e in &Effects::of(std::slice::from_ref(op)) {
            self.add_effect(e);
        }
    }

    /// Add what `effect` touches (design 064 §3.1: the invariant check and the derived views share
    /// one classifier).
    pub fn add_effect(&mut self, effect: &Effect<'_>) {
        match effect {
            Effect::Page { id, .. } => {
                self.page_order = true;
                self.pages.insert(*id);
            }
            Effect::Layer { id, .. } => {
                self.layers.insert(*id);
            }
            Effect::Item { id, .. } => {
                self.items.insert(*id);
            }
            Effect::Device { id, .. } | Effect::DeviceDefinition { id, .. } => {
                self.devices.insert(*id);
            }
            Effect::Terminal { id, .. } => {
                self.terminals.insert(*id);
            }
            Effect::Rep { id, before, after } => {
                self.representations.insert(*id);
                // A cable reference places named points (design 072 §4.4, F4): the routes landing
                // on the points it placed or places are re-checked (I23).
                for r in [before, after].into_iter().flatten().filter(|r| matches!(r.subject, Subject::Cable(_))) {
                    self.terminals.extend(r.connection_points.keys().copied());
                }
            }
            Effect::Wire { id, .. } => {
                self.wires.insert(*id);
            }
            Effect::Cable { id, before, after } => {
                self.cables.insert(*id);
                // Its cores' named points (F4): the routes landing on them are re-checked (I23).
                for c in [before, after].into_iter().flatten() {
                    self.terminals.extend(c.cores.iter().flat_map(|m| m.through.iter().copied()));
                }
            }
            Effect::Route { id, .. } => {
                self.routes.insert(*id);
            }
            Effect::Aid { id, .. } => {
                self.aids.insert(*id);
            }
            Effect::Prop { object, .. } => match object {
                ObjectRef::Device(id) => {
                    self.devices.insert(*id);
                }
                ObjectRef::Wire(id) => {
                    self.wires.insert(*id);
                }
                ObjectRef::Cable(id) => {
                    self.cables.insert(*id);
                }
                ObjectRef::Page(id) => {
                    self.page_properties.insert(*id);
                }
                ObjectRef::Project | ObjectRef::Standard => {}
            },
            Effect::Definition { id, .. } => {
                self.definitions.insert(*id);
            }
            Effect::Settings { .. } | Effect::Version { .. } => {}
            // Links are checked with the project (I24).
            Effect::DeviceLink { .. } => {
                self.project = true;
            }
            Effect::Folder { id, index_before, index_after, .. } => {
                // An add or a remove changes the folder order; a change in place does not.
                if index_before.is_some() || index_after.is_some() {
                    self.folder_order = true;
                }
                self.folders.insert(*id);
            }
            Effect::Style { .. } | Effect::Standard { .. } => self.styles = true,
            Effect::Project { .. } => self.project = true,
        }
    }
}

fn bad(m: String) -> Result<()> {
    Err(ModelError::Invariant(m))
}

impl Document {
    /// Verify every invariant (I1 to I18 in [`crate::invariant`]).
    ///
    /// # Errors
    /// [`ModelError::Invariant`] naming the first violation found.
    pub fn check(&self) -> Result<()> {
        self.check_page_order()?;
        for (id, item) in &self.items {
            self.check_item(*id, item)?;
        }
        for (id, d) in &self.definitions {
            check_definition(*id, d)?;
        }
        for (id, t) in &self.terminals {
            self.check_terminal(*id, t)?;
        }
        for (id, c) in &self.devices {
            self.check_component(*id, c)?;
        }
        let mut owner: BTreeMap<ItemId, RepresentationId> = BTreeMap::new();
        for (id, r) in &self.representations {
            let id = *id;
            self.check_representation(id, r, |item| match owner.insert(item, id) {
                Some(other) => bad(format!("item {item} belongs to representations {other} and {id} (DOC-05)")),
                None => Ok(()),
            })?;
        }
        for (id, c) in &self.wires {
            self.check_conductor(*id, c)?;
        }
        for (id, c) in &self.cables {
            self.check_cable(*id, c)?;
        }
        for (id, r) in &self.routes {
            self.check_route(*id, r)?;
        }
        for (id, a) in &self.aids {
            self.check_aid(*id, a)?;
        }
        self.check_folder_order()?;
        for id in self.folders.keys() {
            self.check_folder(*id)?;
        }
        for id in self.pages.keys() {
            self.check_page_folder(*id)?;
        }
        self.check_styles()?;
        self.check_project()?;
        Ok(())
    }

    /// I22 (design 072 §5.3): the location and function lists are well formed
    /// ([`crate::project::Project::problem`]). A page or the project naming an id with no entry
    /// is not a breach: it shows `?`.
    ///
    /// I25: a drawing aid is well formed.
    fn check_aid(&self, id: crate::id::AidId, a: &crate::aid::Aid) -> Result<()> {
        match a.problem() {
            Some(e) => bad(format!("aid {id}: {e}")),
            None => Ok(()),
        }
    }

    /// I24 (design 072 §2.8, §2.6): every device's user values name a field the project defines
    /// and are of its kind; every device link joins two different devices that exist, once.
    fn check_project(&self) -> Result<()> {
        if let Some(p) = self.project.problem() {
            return bad(p);
        }
        for (id, d) in self.devices.iter().filter(|(_, d)| !d.user.is_empty()) {
            self.check_user_values(*id, d)?;
        }
        self.check_device_links()
    }

    fn check_user_values(&self, id: DeviceId, c: &Device) -> Result<()> {
        for (f, v) in &c.user {
            match self.project.user_field(*f) {
                None => return bad(format!("device {id} has a value of user field {f}, which the project does not define")),
                Some(def) if !v.fits(&def.kind) => {
                    return bad(format!("device {id}'s value of user field \"{}\" is not a {} value", def.name.as_str(), def.kind.word()));
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    fn check_device_links(&self) -> Result<()> {
        let links: Vec<_> = self.device_links.iter().collect();
        for (i, (id, l)) in links.iter().enumerate() {
            if l.a == l.b || !self.devices.contains_key(&l.a) || !self.devices.contains_key(&l.b) {
                return bad(format!("device link {id} does not join two different devices that exist"));
            }
            if links.get(..i).unwrap_or_default().iter().any(|(_, k)| k.same_as(l)) {
                return bad(format!("device link {id} repeats another link"));
            }
        }
        Ok(())
    }

    /// I-styles (brief 050a, design 050 §3.3): every catalogue word is well formed, and no
    /// standard-layer record exists without a standard (the op refuses that, so this holds by
    /// construction). A dangling style reference is not a breach: it falls through.
    fn check_styles(&self) -> Result<()> {
        let layers = std::iter::once(&self.styles).chain(self.standard.as_ref().map(|s| &s.styles));
        for cats in layers {
            for (word, styles) in cats {
                if styles.is_empty() {
                    return bad(format!("style catalogue {word} is listed with no styles"));
                }
            }
        }
        Ok(())
    }

    /// Verify the invariants that the change described by `touched` could have broken, assuming
    /// they all held before it.
    ///
    /// # Semantics
    /// Checks the same predicates as [`Document::check`], for the touched objects and the objects
    /// that depend on them (the table in [`crate::invariant`]). When the state before the change
    /// was valid, it passes exactly when [`Document::check`] passes; the error it reports may name
    /// a different violation when there are several.
    ///
    /// # Errors
    /// [`ModelError::Invariant`] naming a violation.
    pub fn check_touched(&self, touched: &Touched) -> Result<()> {
        let ix = &self.index;
        if touched.page_order {
            self.check_page_order()?;
        }

        let mut items = touched.items.clone();
        for p in &touched.pages {
            items.extend(referrers(&ix.page_items, p));
        }
        for l in &touched.layers {
            items.extend(referrers(&ix.layer_items, l));
        }
        for d in &touched.definitions {
            items.extend(referrers(&ix.definition_items, d));
        }
        for id in &items {
            if let Some(item) = self.items.get(id) {
                self.check_item(*id, item)?;
            }
        }

        for id in &touched.definitions {
            if let Some(d) = self.definitions.get(id) {
                check_definition(*id, d)?;
            }
        }

        let mut terminals = touched.terminals.clone();
        for c in &touched.devices {
            terminals.extend(referrers(&ix.component_terminals, &crate::doc::TerminalOwner::Device(*c)));
        }
        for id in &terminals {
            if let Some(t) = self.terminals.get(id) {
                self.check_terminal(*id, t)?;
            }
        }

        let mut devices = touched.devices.clone();
        for d in &touched.definitions {
            devices.extend(referrers(&ix.definition_components, d));
        }
        for id in &devices {
            if let Some(c) = self.devices.get(id) {
                self.check_component(*id, c)?;
            }
        }

        let mut reps = touched.representations.clone();
        let subjects = (touched.devices.iter().map(|c| Subject::Device(*c)))
            .chain(touched.wires.iter().map(|c| Subject::Wire(*c)))
            .chain(touched.cables.iter().map(|c| Subject::Cable(*c)))
            .chain(touched.routes.iter().map(|c| Subject::Route(*c)))
            .chain(touched.aids.iter().map(|c| Subject::Aid(*c)));
        for s in subjects {
            reps.extend(referrers(&ix.subject_reps, &s));
        }
        for p in &touched.pages {
            reps.extend(referrers(&ix.page_reps, p));
        }
        for i in &touched.items {
            reps.extend(referrers(&ix.item_reps, i));
        }
        for t in &touched.terminals {
            reps.extend(referrers(&ix.terminal_reps, t));
        }
        for id in &reps {
            if let Some(r) = self.representations.get(id) {
                let id = *id;
                self.check_representation(id, r, |item| {
                    if ix.item_listings(item) > 1 {
                        bad(format!("item {item} is listed more than once, by {id} and others (DOC-05)"))
                    } else {
                        Ok(())
                    }
                })?;
            }
        }

        let mut conductors = touched.wires.clone();
        for t in &touched.terminals {
            conductors.extend(referrers(&ix.terminal_ends, t).map(|(c, _)| c));
        }
        for id in &conductors {
            if let Some(c) = self.wires.get(id) {
                self.check_conductor(*id, c)?;
            }
        }

        let mut cables = touched.cables.clone();
        for c in &touched.wires {
            cables.extend(referrers(&ix.conductor_cables, c));
        }
        for r in &touched.routes {
            cables.extend(referrers(&ix.route_cables, r));
        }
        for id in &cables {
            if let Some(c) = self.cables.get(id) {
                self.check_cable(*id, c)?;
            }
        }

        let mut routes = touched.routes.clone();
        for p in &touched.pages {
            routes.extend(referrers(&ix.page_routes, p));
        }
        for t in &touched.terminals {
            routes.extend(referrers(&ix.terminal_routes, t));
        }
        for r in &touched.routes {
            routes.extend(referrers(&ix.route_tees, r));
        }
        for id in &routes {
            if let Some(r) = self.routes.get(id) {
                self.check_route(*id, r)?;
            }
        }
        for id in &touched.aids {
            if let Some(a) = self.aids.get(id) {
                self.check_aid(*id, a)?;
            }
        }

        if touched.folder_order {
            self.check_folder_order()?;
        }
        // A folder's depth depends on its parents, so its sub-folders (all levels) are checked too.
        let mut folders: BTreeSet<FolderId> = BTreeSet::new();
        let mut todo: Vec<FolderId> = touched.folders.iter().copied().collect();
        while let Some(f) = todo.pop() {
            if folders.insert(f) {
                todo.extend(referrers(&ix.folder_children, &f));
            }
            // The index is kept for any state, so a folder count bounds the walk even for a cycle.
            if folders.len() > self.folders.len() + touched.folders.len() {
                break;
            }
        }
        for id in &folders {
            if self.folders.contains_key(id) {
                self.check_folder(*id)?;
            }
        }
        let mut pages: BTreeSet<PageId> = touched.pages.union(&touched.page_properties).copied().collect();
        for f in &touched.folders {
            pages.extend(referrers(&ix.folder_pages, f));
        }
        for id in &pages {
            if self.pages.contains_key(id) {
                self.check_page_folder(*id)?;
            }
        }
        if touched.styles {
            self.check_styles()?;
        }
        if touched.project {
            self.check_project()?;
        }
        Ok(())
    }

    /// I19.
    fn check_folder_order(&self) -> Result<()> {
        let order: BTreeSet<_> = self.folder_order.iter().collect();
        if order.len() != self.folder_order.len() || order.len() != self.folders.len() || !self.folders.keys().all(|f| order.contains(f)) {
            return bad("folder order does not list each folder exactly once".into());
        }
        Ok(())
    }

    /// I20.
    fn check_folder(&self, id: FolderId) -> Result<()> {
        if crate::book::folder_depth(self, id).is_none() {
            return bad(format!("folder {id} has a missing parent, is inside itself or nests more than {} deep", crate::book::MAX_FOLDER_DEPTH));
        }
        Ok(())
    }

    /// I21.
    fn check_page_folder(&self, id: PageId) -> Result<()> {
        match self.pages.get(&id).and_then(|p| p.folder) {
            Some(f) if !self.folders.contains_key(&f) => bad(format!("page {id} names a missing folder")),
            Some(_) | None => Ok(()),
        }
    }

    /// I1.
    fn check_page_order(&self) -> Result<()> {
        let order: BTreeSet<_> = self.page_order.iter().collect();
        if order.len() != self.page_order.len() || order.len() != self.pages.len() || !self.pages.keys().all(|p| order.contains(p)) {
            return bad("page order does not list each page exactly once".into());
        }
        Ok(())
    }

    /// I2 to I6.
    fn check_item(&self, id: ItemId, item: &Item) -> Result<()> {
        if !self.pages.contains_key(&item.page) {
            return bad(format!("item {id} is on a missing page"));
        }
        if !self.layers.contains_key(&item.style.layer) {
            return bad(format!("item {id} uses a missing layer"));
        }
        match &item.kind {
            ItemKind::Primitive(p) if !p.is_finite() => bad(format!("item {id} has non-finite geometry")),
            ItemKind::Symbol(s) if !self.definitions.contains_key(&s.definition) => bad(format!("item {id} places a missing definition")),
            ItemKind::Symbol(s) if !s.placement.is_valid() => bad(format!("item {id} has an invalid placement")),
            _ => Ok(()),
        }
    }

    /// I8.
    fn check_terminal(&self, id: TerminalId, t: &Terminal) -> Result<()> {
        let device = match t.owner {
            crate::doc::TerminalOwner::Device(d) => d,
            crate::doc::TerminalOwner::Aid(a) => {
                return match self.aids.get(&a) {
                    Some(crate::aid::Aid::Continuation { .. }) => Ok(()),
                    Some(_) => bad(format!("terminal {id} belongs to drawing aid {a}, which is not a continuation")),
                    None => bad(format!("terminal {id} belongs to a missing drawing aid")),
                };
            }
        };
        let Some(c) = self.devices.get(&device) else {
            return bad(format!("terminal {id} belongs to a missing component"));
        };
        // Design 072 §3: a PLC module's point is a channel's (`.3.in`) or one of its own symbol's
        // (`L+`). Any other definition key (an old `I1.3`) is unknown: nothing reads a channel
        // out of its text.
        if let crate::device::DeviceType::PlcModule(_) = &c.device_type
            && let Some(crate::PointKey::Def(k)) = &t.key
            && !c.definition.and_then(|d| self.definitions.get(&d)).is_some_and(|d| d.terminals.iter().any(|td| td.key.as_def() == Some(k)))
        {
            return bad(format!("terminal {id} of PLC module {} is keyed {k}: neither a channel point nor a point of its symbol", device));
        }
        Ok(())
    }

    /// I9.
    fn check_component(&self, id: DeviceId, c: &Device) -> Result<()> {
        if let Some(d) = c.definition
            && !self.definitions.contains_key(&d)
        {
            return bad(format!("device {id} uses a missing definition"));
        }
        // Design 072 §2.1: a key that became a device field is never in the device's bag.
        if let Some(k) = c.properties.keys().find(|k| crate::keys::is_reserved_on(crate::keys::Holder::Device, k)) {
            return bad(format!("device {id} holds {k} in its properties; it is a field"));
        }
        self.check_user_values(id, c)?;
        if let Some(p) = c.device_type.problem() {
            return bad(format!("device {id}: {p}"));
        }
        // Design 072 §2.2 and §2.5: a protective device's ratings (a fuse bank's per output) and a
        // strip's fuse position ratings are fields.
        if let crate::device::DeviceType::Protection(_) | crate::device::DeviceType::TerminalStrip(_) = &c.device_type
            && let Some(k) = c.properties.keys().find(|k| k.starts_with(crate::device::RATING_OF_PREFIX))
        {
            return bad(format!("device {id} holds {k} in its properties; it is a field"));
        }
        Ok(())
    }

    /// I23: a route is well formed, on an existing page, and lands on existing connection points
    /// and on existing routes of its own page.
    fn check_route(&self, id: RouteId, r: &crate::route::Route) -> Result<()> {
        if let Err(e) = r.check(id) {
            return bad(format!("route {id}: {e}"));
        }
        if !self.pages.contains_key(&r.page) {
            return bad(format!("route {id} is on a missing page"));
        }
        for e in r.ends {
            match e {
                crate::route::RouteEnd::Point(t) if !self.has_point(t) => {
                    return bad(format!("route {id} lands on a missing connection point {t}"));
                }
                crate::route::RouteEnd::On { route, .. } if self.routes.get(&route).is_none_or(|o| o.page != r.page) => {
                    return bad(format!("route {id} lands on route {route}, which is not on its page"));
                }
                crate::route::RouteEnd::Point(_) | crate::route::RouteEnd::On { .. } | crate::route::RouteEnd::Free => {}
            }
        }
        Ok(())
    }

    /// I10 to I16. `doc05` is called for each listed item, in order, and decides I13.
    fn check_representation(&self, id: RepresentationId, r: &Representation, mut doc05: impl FnMut(ItemId) -> Result<()>) -> Result<()> {
        let subject_exists = match r.subject {
            Subject::Device(c) => self.devices.contains_key(&c),
            Subject::Wire(c) => self.wires.contains_key(&c),
            Subject::Cable(c) => self.cables.contains_key(&c),
            Subject::Route(c) => self.routes.contains_key(&c),
            Subject::Aid(c) => self.aids.contains_key(&c),
        };
        if !subject_exists {
            return bad(format!("representation {id} depicts a missing subject"));
        }
        if !self.pages.contains_key(&r.page) {
            return bad(format!("representation {id} is on a missing page"));
        }
        for item in &r.items {
            let Some(it) = self.items.get(item) else {
                return bad(format!("representation {id} lists missing item {item}"));
            };
            if it.page != r.page {
                return bad(format!("representation {id} lists item {item} from another page"));
            }
            doc05(*item)?;
        }
        match r.form {
            Form::Symbol => {
                let instance = match r.items.as_slice() {
                    [only] => match self.items.get(only).map(|i| &i.kind) {
                        Some(ItemKind::Symbol(inst)) => Some(inst),
                        _ => None,
                    },
                    _ => None,
                };
                let Some(inst) = instance else {
                    return bad(format!("symbol representation {id} must hold exactly one symbol instance"));
                };
                match r.subject {
                    // Without connection points its terminals are matched to the definition by key,
                    // so the symbol shown must be the component's own definition. With them (one
                    // device drawn as a coil and its contacts, brief 018), any embedded definition
                    // (I5).
                    Subject::Device(c) => {
                        if r.connection_points.is_empty() && self.devices.get(&c).and_then(|c| c.definition) != Some(inst.definition) {
                            return bad(format!("symbol representation {id} shows a definition its component does not use"));
                        }
                    }
                    // A continuation's arrow (design 072 §6.2): an aid has no definition of its own.
                    Subject::Aid(a) if matches!(self.aids.get(&a), Some(crate::aid::Aid::Continuation { .. })) => {}
                    _ => return bad(format!("symbol representation {id} must depict a component or a continuation")),
                }
            }
            Form::Drawn => {
                if r.items.iter().filter_map(|i| self.items.get(i)).any(|i| matches!(i.kind, ItemKind::Symbol(_))) {
                    return bad(format!("drawn representation {id} holds a symbol instance"));
                }
            }
        }
        for (t, p) in &r.connection_points {
            // A cable reference places the named points of its cable's cores (design 072 §4.4, F4).
            if let Subject::Cable(k) = r.subject {
                if r.role != crate::Role::Reference || self.cable_point(*t).map(|(c, _)| c) != Some(k) {
                    return bad(format!("representation {id} places point {t}, which is not a point of its cable's cores"));
                }
                if !p.is_finite() {
                    return bad(format!("representation {id} has a non-finite connection point"));
                }
                continue;
            }
            if !matches!(r.subject, Subject::Device(_) | Subject::Aid(_)) {
                return bad(format!("representation {id} of a non-component has connection points"));
            }
            if self.terminals.get(t).map(|t| t.owner.subject()) != Some(r.subject) {
                return bad(format!("representation {id} places terminal {t} of another component"));
            }
            if !p.is_finite() {
                return bad(format!("representation {id} has a non-finite connection point"));
            }
        }
        Ok(())
    }

    /// I17.
    fn check_conductor(&self, id: WireId, c: &Wire) -> Result<()> {
        for t in &c.ends {
            if !self.terminals.contains_key(t) {
                return bad(format!("wire {id} is attached to missing terminal {t}"));
            }
        }
        if c.ends[0] == c.ends[1] {
            return bad(format!("wire {id} ends twice at terminal {}", c.ends[0]));
        }
        if c.net_order == 0 {
            return bad(format!("wire {id} has no place in its net's order"));
        }
        Ok(())
    }

    /// I18.
    fn check_cable(&self, id: CableId, c: &Cable) -> Result<()> {
        // Design 072 §4.5: a key that became a cable field is never in the cable's bag.
        if let Some(k) = c.properties.keys().find(|k| crate::keys::is_reserved_on(crate::keys::Holder::Cable, k)) {
            return bad(format!("cable {id} holds {k} in its properties; it is a field"));
        }
        let mut seen = BTreeSet::new();
        let mut points = BTreeSet::new();
        for m in &c.cores {
            // Design 072 §4.4 (F4): each named point is one core's, placed by one reference of the cable.
            for t in &m.through {
                if !points.insert(*t) || self.terminals.contains_key(t) {
                    return bad(format!("cable {id} lists point {t} twice, or a component's terminal as a named point"));
                }
                let placed =
                    self.representations_showing(*t).filter(|(_, r)| r.subject == Subject::Cable(id) && r.role == crate::Role::Reference).count();
                if placed != 1 {
                    return bad(format!("cable {id}'s point {t} is placed by {placed} references, not one"));
                }
            }
            match m.position {
                crate::doc::CorePosition::Unplaced => continue,
                crate::doc::CorePosition::Wire(w) if !self.wires.contains_key(&w) => {
                    return bad(format!("cable {id} lists missing conductor {w}"));
                }
                crate::doc::CorePosition::Route { route, .. } if !self.routes.contains_key(&route) => {
                    return bad(format!("cable {id} has a core on missing route {route}"));
                }
                crate::doc::CorePosition::Route { landed: Some(t), .. } if !self.has_point(t) => {
                    return bad(format!("cable {id} has a core landed at missing point {t}"));
                }
                crate::doc::CorePosition::Wire(_) | crate::doc::CorePosition::Route { .. } => {}
            }
            if !seen.insert(m.position) {
                return bad(format!("cable {id} lists {:?} twice", m.position));
            }
        }
        Ok(())
    }
}

/// I7.
fn check_definition(id: DefinitionId, d: &Definition) -> Result<()> {
    if !d.graphics.iter().all(Primitive::is_finite) || !d.terminals.iter().all(|t| t.at.is_finite()) {
        return bad(format!("definition {id} has non-finite geometry"));
    }
    let keys: BTreeSet<&PointKey> = d.terminals.iter().map(|t| &t.key).collect();
    if keys.len() != d.terminals.len() {
        return bad(format!("definition {id} repeats a terminal key"));
    }
    // Brief 044 (overseer condition on FP2): a bridge names keys a placement can write.
    if let Some(k) = d.bridges.iter().flatten().find(|k| !keys.contains(k) && !is_placement_key(k)) {
        return bad(format!("definition {id} bridges key {k}, which it has no terminal for and no placement writes"));
    }
    // Brief 048 (FP1, FP2, FP4).
    let mut texts = BTreeSet::new();
    for slot in &d.texts {
        if !slot.at.is_finite() || !slot.rotation.is_finite() {
            return bad(format!("definition {id} has a text slot at a non-finite position"));
        }
        if !texts.insert(&slot.text) {
            return bad(format!("definition {id} has two slots for {:?}", slot.text.word()));
        }
    }
    if let (Some(from), Some(own)) = (&d.derived_from, &d.origin)
        && from.library == own.library
        && from.definition == own.definition
        && from.library_seq == own.library_seq
    {
        return bad(format!("definition {id} is derived from itself"));
    }
    Ok(())
}

/// A terminal key a placement writes that need not be one of the definition's own keys (brief
/// 044, FP2): a connector side key (`.s`, `.p`) or a strip level key (`.1`, `.2`, `.1i`, brief
/// 036). A bridge may name these; any other key it names must be one of the definition's own.
pub fn is_placement_key(k: &PointKey) -> bool {
    matches!(k, PointKey::Side(_)) || (matches!(k, PointKey::Level { .. }) && crate::links::level_of_key(Some(k)).is_some())
}
