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
//! | I7 | A definition's geometry is finite, its terminal keys are unique, and each bridge key is one of them or a placement key (`.s`, `.p`, a level key; brief 044) | definition | touched definitions |
//! | I8 | A terminal's component exists (DOC-07) | terminal → component | touched terminals; terminals of a touched component |
//! | I9 | A component's definition exists (DOC-07) | component → definition | touched components; components using a touched definition |
//! | I10 | A representation's subject exists (DOC-07) | representation → component, conductor or cable | touched representations; representations of a touched subject |
//! | I11 | A representation's page exists | representation → page | touched representations; representations on a touched page |
//! | I12 | A representation's items exist and lie on its page | representation → items | touched representations; representations listing a touched item |
//! | I13 | An item is listed once, by at most one representation (DOC-05) | representation ↔ representation, through an item | touched representations (all listings of each of their items are counted); representations listing a touched item |
//! | I14 | A symbol representation holds exactly one symbol instance and depicts a component; without connection points it shows that component's own definition (with them, any embedded definition: decision 2026-11-08, brief 018) | representation → item, representation → component → definition | touched representations; representations listing a touched item; representations of a touched component |
//! | I15 | A drawn representation holds no symbol instance | representation → items | touched representations; representations listing a touched item |
//! | I16 | Connection points belong to a component representation, name that component's terminals and are finite | representation → terminals → component | touched representations; representations placing a touched terminal; representations of a touched component |
//! | I17 | Conductor ends attach to existing terminals (DOC-03, DOC-07) | conductor → terminal | touched conductors; conductors attached to a touched terminal |
//! | I18 | Cable members exist and are listed once (DOC-04, DOC-07) | cable → conductors | touched cables; cables listing a touched conductor |
//! | I19 | The folder order lists each folder exactly once (brief 019) | folders, folder order | any folder added or removed (whole order, which is short) |
//! | I20 | A folder's parent exists, the parents form no cycle and it nests at most 8 deep | folder → parent folders | touched folders, their sub-folders (all levels) and the sub-folders of a removed one |
//! | I21 | A page's `folder` property names an existing folder | page → folder | touched pages and pages whose properties changed; pages in a touched folder |
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

use crate::doc::{Cable, Component, Conductor, Definition, Document, Form, Item, ItemKind, Primitive, Representation, Subject, Terminal};
use crate::error::{ModelError, Result};
use crate::id::{CableId, ComponentId, ConductorId, DefinitionId, FolderId, ItemId, LayerId, PageId, RepresentationId, TerminalId};
use crate::index::referrers;
use crate::op::{ObjectRef, Op};

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
    pub components: BTreeSet<ComponentId>,
    pub terminals: BTreeSet<TerminalId>,
    pub representations: BTreeSet<RepresentationId>,
    pub conductors: BTreeSet<ConductorId>,
    pub cables: BTreeSet<CableId>,
    pub definitions: BTreeSet<DefinitionId>,
    /// A folder was added or removed, so the folder order changed.
    pub folder_order: bool,
    pub folders: BTreeSet<FolderId>,
    /// Pages whose properties changed (only I21 depends on them).
    pub page_properties: BTreeSet<PageId>,
}

impl Touched {
    /// What `ops` touch, together.
    pub fn of(ops: &[Op]) -> Self {
        let mut t = Touched::default();
        for op in ops {
            t.add(op);
        }
        t
    }

    /// Add what `op` touches.
    pub fn add(&mut self, op: &Op) {
        match op {
            Op::AddPage { id, .. } | Op::RemovePage { id, .. } => {
                self.page_order = true;
                self.pages.insert(*id);
            }
            Op::AddLayer { id, .. } | Op::RemoveLayer { id, .. } => {
                self.layers.insert(*id);
            }
            Op::AddItem { id, .. } | Op::ModifyItem { id, .. } | Op::RemoveItem { id, .. } => {
                self.items.insert(*id);
            }
            Op::CreateComponent { id, .. } | Op::DeleteComponent { id, .. } => {
                self.components.insert(*id);
            }
            Op::AddTerminal { id, .. } | Op::RemoveTerminal { id, .. } => {
                self.terminals.insert(*id);
            }
            Op::LinkRepresentation { id, .. } | Op::ModifyRepresentation { id, .. } | Op::UnlinkRepresentation { id, .. } => {
                self.representations.insert(*id);
            }
            Op::CreateConductor { id, .. } | Op::DeleteConductor { id, .. } => {
                self.conductors.insert(*id);
            }
            Op::AttachEnd { conductor, .. } => {
                self.conductors.insert(*conductor);
            }
            Op::CreateCable { id, .. } | Op::DeleteCable { id, .. } => {
                self.cables.insert(*id);
            }
            Op::AddCableMember { cable, .. } | Op::RemoveCableMember { cable, .. } => {
                self.cables.insert(*cable);
            }
            Op::SetProperty { object, .. } => match object {
                ObjectRef::Component(id) => {
                    self.components.insert(*id);
                }
                ObjectRef::Conductor(id) => {
                    self.conductors.insert(*id);
                }
                ObjectRef::Cable(id) => {
                    self.cables.insert(*id);
                }
                ObjectRef::Page(id) => {
                    self.page_properties.insert(*id);
                }
                ObjectRef::Project => {}
            },
            Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => {
                self.definitions.insert(*id);
            }
            Op::SetProjectSettings { .. } | Op::NameVersion { .. } | Op::UnnameVersion { .. } => {}
            Op::AddFolder { id, .. } | Op::RemoveFolder { id, .. } => {
                self.folder_order = true;
                self.folders.insert(*id);
            }
            Op::ModifyFolder { id, .. } => {
                self.folders.insert(*id);
            }
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
        for (id, c) in &self.components {
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
        for (id, c) in &self.conductors {
            self.check_conductor(*id, c)?;
        }
        for (id, c) in &self.cables {
            self.check_cable(*id, c)?;
        }
        self.check_folder_order()?;
        for id in self.folders.keys() {
            self.check_folder(*id)?;
        }
        for id in self.pages.keys() {
            self.check_page_folder(*id)?;
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
        for c in &touched.components {
            terminals.extend(referrers(&ix.component_terminals, c));
        }
        for id in &terminals {
            if let Some(t) = self.terminals.get(id) {
                self.check_terminal(*id, t)?;
            }
        }

        let mut components = touched.components.clone();
        for d in &touched.definitions {
            components.extend(referrers(&ix.definition_components, d));
        }
        for id in &components {
            if let Some(c) = self.components.get(id) {
                self.check_component(*id, c)?;
            }
        }

        let mut reps = touched.representations.clone();
        let subjects = (touched.components.iter().map(|c| Subject::Component(*c)))
            .chain(touched.conductors.iter().map(|c| Subject::Conductor(*c)))
            .chain(touched.cables.iter().map(|c| Subject::Cable(*c)));
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

        let mut conductors = touched.conductors.clone();
        for t in &touched.terminals {
            conductors.extend(referrers(&ix.terminal_ends, t).map(|(c, _)| c));
        }
        for id in &conductors {
            if let Some(c) = self.conductors.get(id) {
                self.check_conductor(*id, c)?;
            }
        }

        let mut cables = touched.cables.clone();
        for c in &touched.conductors {
            cables.extend(referrers(&ix.conductor_cables, c));
        }
        for id in &cables {
            if let Some(c) = self.cables.get(id) {
                self.check_cable(*id, c)?;
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
        let Some(text) = self.pages.get(&id).and_then(|p| p.properties.get(crate::book::FOLDER_KEY)) else {
            return Ok(());
        };
        match FolderId::parse(text) {
            Some(f) if self.folders.contains_key(&f) => Ok(()),
            _ => bad(format!("page {id} names a missing folder")),
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
        if !self.components.contains_key(&t.component) {
            return bad(format!("terminal {id} belongs to a missing component"));
        }
        Ok(())
    }

    /// I9.
    fn check_component(&self, id: ComponentId, c: &Component) -> Result<()> {
        if let Some(d) = c.definition
            && !self.definitions.contains_key(&d)
        {
            return bad(format!("component {id} uses a missing definition"));
        }
        Ok(())
    }

    /// I10 to I16. `doc05` is called for each listed item, in order, and decides I13.
    fn check_representation(&self, id: RepresentationId, r: &Representation, mut doc05: impl FnMut(ItemId) -> Result<()>) -> Result<()> {
        let subject_exists = match r.subject {
            Subject::Component(c) => self.components.contains_key(&c),
            Subject::Conductor(c) => self.conductors.contains_key(&c),
            Subject::Cable(c) => self.cables.contains_key(&c),
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
                let Subject::Component(c) = r.subject else {
                    return bad(format!("symbol representation {id} must depict a component"));
                };
                // Without connection points its terminals are matched to the definition by key, so
                // the symbol shown must be the component's own definition. With them (one device
                // drawn as a coil and its contacts, brief 018), any embedded definition (I5).
                if r.connection_points.is_empty() && self.components.get(&c).and_then(|c| c.definition) != Some(inst.definition) {
                    return bad(format!("symbol representation {id} shows a definition its component does not use"));
                }
            }
            Form::Drawn => {
                if r.items.iter().filter_map(|i| self.items.get(i)).any(|i| matches!(i.kind, ItemKind::Symbol(_))) {
                    return bad(format!("drawn representation {id} holds a symbol instance"));
                }
            }
        }
        for (t, p) in &r.connection_points {
            let Subject::Component(c) = r.subject else {
                return bad(format!("representation {id} of a non-component has connection points"));
            };
            if self.terminals.get(t).map(|t| t.component) != Some(c) {
                return bad(format!("representation {id} places terminal {t} of another component"));
            }
            if !p.is_finite() {
                return bad(format!("representation {id} has a non-finite connection point"));
            }
        }
        Ok(())
    }

    /// I17.
    fn check_conductor(&self, id: ConductorId, c: &Conductor) -> Result<()> {
        for t in c.ends.iter().flatten() {
            if !self.terminals.contains_key(t) {
                return bad(format!("conductor {id} is attached to missing terminal {t}"));
            }
        }
        Ok(())
    }

    /// I18.
    fn check_cable(&self, id: CableId, c: &Cable) -> Result<()> {
        let mut seen = BTreeSet::new();
        for m in &c.members {
            if !self.conductors.contains_key(&m.conductor) {
                return bad(format!("cable {id} lists missing conductor {}", m.conductor));
            }
            if !seen.insert(m.conductor) {
                return bad(format!("cable {id} lists conductor {} twice", m.conductor));
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
    let keys: BTreeSet<&str> = d.terminals.iter().map(|t| t.key.as_str()).collect();
    if keys.len() != d.terminals.len() {
        return bad(format!("definition {id} repeats a terminal key"));
    }
    // Brief 044 (overseer condition on FP2): a bridge names keys a placement can write.
    if let Some(k) = d.bridges.iter().flatten().find(|k| !keys.contains(k.as_str()) && !is_placement_key(k)) {
        return bad(format!("definition {id} bridges key {k:?}, which it has no terminal for and no placement writes"));
    }
    Ok(())
}

/// A terminal key a placement writes that need not be one of the definition's own keys (brief
/// 044, FP2): a connector side key (`.s`, `.p`) or a strip level key (`.1`, `.2`, `.1i`, brief
/// 036). A bridge may name these; any other key it names must be one of the definition's own.
pub fn is_placement_key(k: &str) -> bool {
    k == ".s" || k == ".p" || (k.starts_with('.') && crate::links::level_of_key(Some(k)).is_some())
}
