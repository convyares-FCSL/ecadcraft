//! What a commit changes in the device references: the devices to compare and the texts to redraw.

use ecad_model::document::effect::Effect;
use ecad_model::look::Look;
use ecad_model::{CableId, DeviceId, Document, ObjectRef, PageId, Representation, Subject};
use std::collections::{BTreeMap, BTreeSet};

use super::classify::Group;
use super::index::RefIndex;
use super::places::Places;
use super::read::{contacts_changed, table_text_key};
use super::table::{Part, shown};

// ---------------------------------------------------------------- per commit

/// What a commit's effects could change in the device references: the devices to compare, or every
/// page with a device member (`wide`).
#[derive(Debug, Clone, Default)]
pub(super) struct Scope {
    devices: BTreeSet<DeviceId>,
    wide: bool,
    /// The continuation names whose arrows' texts and caption runs
    /// are compared, or every page with an arrow (`arrows_wide`).
    names: BTreeSet<String>,
    arrows_wide: bool,
    /// Pages whose device members' devices, and whose arrows' names, are added once the reference
    /// index of the same state is at hand ([`Scope::resolve`]).
    device_pages: BTreeSet<PageId>,
    name_pages: BTreeSet<PageId>,
    /// Text styles changed, weighed once the look is at hand ([`Scope::styles`]): the table
    /// text's style makes the devices wide, the reference or caption text's the arrows.
    text_styles: BTreeSet<ecad_model::StyleRef>,
}

impl Scope {
    /// Add what `effect` names, looked up in `doc` (the state before the commit, or after it).
    /// The pages whose book position changes are added by [`Watch`] after the commit.
    fn note(&mut self, doc: &Document, effect: &Effect<'_>) {
        let ends = |s: &mut Scope, w: ecad_model::WireId| {
            if let Some(c) = doc.wires().get(&w) {
                for t in &c.ends {
                    if let Some(x) = doc.connection_points().get(t) {
                        s.devices.extend(x.device());
                    }
                }
            }
        };
        let rep = |s: &mut Scope, r: &Representation| match r.subject {
            Subject::Device(c) => {
                s.devices.insert(c);
            }
            Subject::Wire(w) => ends(s, w),
            Subject::Cable(_) | Subject::Route(_) | Subject::Aid(_) => {}
        };
        match effect {
            Effect::Item { id, .. } => {
                if let Some((_, r)) = doc.representation_of_item(*id) {
                    rep(self, r);
                }
            }
            Effect::Rep { before, after, .. } => {
                for r in before.iter().chain(after.iter()) {
                    rep(self, r);
                }
            }
            Effect::Wire { before: None, after: Some(wire), .. } | Effect::Wire { before: Some(wire), after: None, .. } => {
                for t in &wire.ends {
                    if let Some(x) = doc.connection_points().get(t) {
                        self.devices.extend(x.device());
                    }
                }
            }
            Effect::ConnectionPoint { before, after, .. } => {
                self.devices.extend(before.iter().chain(after.iter()).filter_map(|t| t.device()));
            }
            Effect::Device { id, before: None, .. } | Effect::Device { id, after: None, .. } | Effect::DeviceDefinition { id, .. } => {
                self.devices.insert(*id);
            }
            // A device's contacts or its contact table (typed fields).
            Effect::Device { id, before: Some(b), after: Some(a) } if contacts_changed(b, a) => {
                self.devices.insert(*id);
            }
            Effect::Page { id: p, before: Some(b), after: Some(a), index_before: None, index_after: None } => {
                let ch = b.change(a);
                if ch.number || ch.sheet || ch.page_type {
                    self.device_pages.insert(*p);
                    self.name_pages.insert(*p);
                }
            }
            // A page type's sheet changes its pages' grids.
            Effect::Style { catalogue: ecad_model::StyleCatalogue::Page, .. } => {
                if let Some((pages, true)) = crate::styles::page::touched_by_effect(doc, effect) {
                    self.device_pages.extend(pages.iter().copied());
                    self.name_pages.extend(pages);
                }
            }
            Effect::Page { .. } | Effect::Folder { .. } => {}
            Effect::Definition { id, .. } => self.devices.extend(doc.components_using(*id)),
            // The table text's size or the font sizes every table.
            Effect::Prop { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if table_text_key(key.raw) => {
                self.wide = true;
            }
            // A change of a text style: the table text's sizes every table, the reference or
            // caption text's every arrow.
            Effect::Style { catalogue: ecad_model::StyleCatalogue::Text, id, .. } => {
                self.text_styles.insert((*id).clone());
            }
            Effect::Settings { before, after }
                if before.reference_style != after.reference_style
                    || before.sheet != after.sheet
                    || before.length_unit != after.length_unit
                    || before.contact_mirror != after.contact_mirror =>
            {
                self.wide = true;
            }
            Effect::Layer { .. }
            | Effect::Device { .. }
            | Effect::DeviceLink { .. }
            | Effect::Wire { .. }
            | Effect::Route { .. }
            | Effect::Cable { .. }
            | Effect::Prop { .. }
            | Effect::Settings { .. }
            | Effect::Version { .. }
            | Effect::Style { .. }
            | Effect::Standard { .. }
            | Effect::Aid { .. }
            | Effect::LoosePart { .. }
            | Effect::Project { .. } => {}
        }
        // A frame decides its pages' grids and the free
        // area above the block, so its pages' devices, tables and arrows follow it.
        match crate::title_blocks::frame_reach_of(doc, effect) {
            crate::title_blocks::Reach::Pages(ps) => {
                self.device_pages.extend(ps.iter().copied());
                self.name_pages.extend(ps);
            }
            crate::title_blocks::Reach::Wide => {
                self.wide = true;
                self.arrows_wide = true;
            }
            crate::title_blocks::Reach::NotFrame | crate::title_blocks::Reach::Unchanged => {}
        }
        // What continuation arrows the effect could change.
        match effect {
            Effect::Settings { before, after }
                if before.reference_style != after.reference_style || before.sheet != after.sheet || before.length_unit != after.length_unit =>
            {
                self.arrows_wide = true;
            }
            Effect::Prop { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if crate::continuations::is_wide_key(key.raw) => {
                self.arrows_wide = true;
            }
            Effect::Page { .. }
            | Effect::Layer { .. }
            | Effect::Item { .. }
            | Effect::Device { .. }
            | Effect::DeviceDefinition { .. }
            | Effect::ConnectionPoint { .. }
            | Effect::Rep { .. }
            | Effect::Wire { .. }
            | Effect::Route { .. }
            | Effect::Cable { .. }
            | Effect::Prop { .. }
            | Effect::Definition { .. }
            | Effect::Settings { .. }
            | Effect::Version { .. }
            | Effect::Folder { .. }
            | Effect::Style { .. }
            | Effect::Standard { .. }
            | Effect::Project { .. }
            | Effect::Aid { .. }
            | Effect::LoosePart { .. }
            | Effect::DeviceLink { .. } => {}
        }
        crate::continuations::names_named_by(doc, effect, &mut self.names);
    }

    /// Weigh the text styles noted so far in `m`: the table text's makes the devices wide, the
    /// reference or caption text's the arrows.
    fn styles(&mut self, doc: &Document, m: &dyn Look) {
        let at = ecad_model::registry::Scope::project();
        for id in std::mem::take(&mut self.text_styles) {
            if id == m.style_ref(doc, ecad_model::look::TextKind::Table, at) {
                self.wide = true;
            }
            if crate::continuations::is_wide_style(doc, m, &id) {
                self.arrows_wide = true;
            }
        }
    }

    /// The devices with a member, and the names of the arrows, on the pages noted so far, through
    /// `idx`, the reference index of `doc`'s state.
    fn resolve(&mut self, doc: &Document, idx: &RefIndex, m: &dyn Look) {
        self.styles(doc, m);
        for p in std::mem::take(&mut self.device_pages) {
            self.devices.extend(idx.devices_on(doc, p));
        }
        for p in std::mem::take(&mut self.name_pages) {
            self.names.extend(crate::continuations::names_on(doc, idx, p));
        }
    }
}

/// What a commit could change in the references (the references view's scope):
/// noted effect by effect on the document before the commit ([`Watch::note`]), derived on it
/// ([`Watch::take`]), noted again on the document after it, and finished ([`Watch::finish`]):
/// the cable references ([`crate::references::cables::Snapshot`], scoped per cable) and the touched
/// devices' texts and tables.
#[derive(Debug, Clone, Default)]
pub(crate) struct Watch {
    /// The cables whose references are compared, or every cable (`all_cables`).
    cable_scope: BTreeSet<CableId>,
    all_cables: bool,
    cables: Option<crate::references::cables::Snapshot>,
    scope: Scope,
    before: BTreeMap<DeviceId, BTreeMap<PageId, Vec<Part>>>,
    /// The pages with a device member before (only in the wide case).
    wide_pages: BTreeSet<PageId>,
    /// What the named continuations showed before, by page, and the pages with an
    /// arrow before (the wide case).
    before_names: BTreeMap<PageId, Vec<Part>>,
    arrow_pages: BTreeSet<PageId>,
    groups: BTreeSet<Group>,
    /// A page moved to or from a type with checks off changes the
    /// groups of every device on it: the index is made again ([`Watch::rebuilds`]).
    rebuild: bool,
    /// The book positions before the commit, compared with the book after it once
    /// (`moved_done`): the pages whose position changed, their devices, arrows and cables.
    positions: ecad_model::document::book::PositionsNoted,
    moved_done: bool,
    /// What verify mode found wrong (bugs), for the caller to record.
    problems: Vec<String>,
}

/// What a finished [`Watch`] found.
#[derive(Debug, Clone, Default)]
pub(crate) struct Outcome {
    /// Pages whose references changed; `None` for every page.
    pub pages: Option<BTreeSet<PageId>>,
    /// The line about references left without another end.
    pub orphaned: Option<String>,
    /// The texts and tables of the devices it looked at, after the commit: the next commit's
    /// "before" when nothing has changed in between ([`Watch::take`]).
    pub shown: Shown,
    /// What verify mode found wrong and repaired (bugs): one line each, for the event log.
    pub problems: Vec<String>,
}

/// Texts and tables derived for some devices, by page.
pub(crate) type Shown = BTreeMap<DeviceId, BTreeMap<PageId, Vec<Part>>>;

impl Watch {
    /// Note one effect of the transaction. `before` says which side `doc` is: the
    /// state before the commit (the ops given to it) or after it (the transaction as recorded).
    pub(crate) fn note(&mut self, doc: &Document, before: bool, effect: &Effect<'_>) {
        if before {
            if !self.all_cables
                && matches!(
                    crate::references::cables::cables_touched_by(doc, effect, &mut self.cable_scope),
                    crate::references::cables::Touch::All(_)
                )
            {
                self.all_cables = true;
            }
            self.positions.note_before(doc, effect);
            self.scope.note(doc, effect);
        } else {
            if !std::mem::replace(&mut self.moved_done, true) {
                let pages = self.positions.changed(doc);
                if !self.all_cables {
                    for p in &pages {
                        self.cable_scope.extend(crate::references::cables::cables_on(doc, *p));
                    }
                }
                self.scope.device_pages.extend(pages.iter().copied());
                self.scope.name_pages.extend(pages);
            }
            self.scope.note(doc, effect);
            self.rebuild |= crate::pages::page_types::checks_changed(doc, effect);
        }
        RefIndex::groups_named_by(doc, effect, &mut self.groups);
    }

    /// Whether the index must be made again whole (a page type's checks switched).
    pub(crate) fn rebuilds(&self) -> bool {
        self.rebuild
    }

    /// After the Before notes: derive what the noted devices, names and cables show on `doc`, the
    /// state before the commit, with `idx` its index. `reuse` holds what the last commit derived,
    /// for the same `doc` (the caller keeps it only while the state is unchanged): a device found
    /// there is not derived again.
    pub(crate) fn take(&mut self, doc: &Document, idx: &RefIndex, reuse: &Shown, m: &dyn Look) {
        self.scope.resolve(doc, idx, m);
        let scope = (!self.all_cables).then_some(&self.cable_scope);
        self.cables = Some(crate::references::cables::Snapshot::take(doc, scope));
        let mut places = Places::new(doc);
        if self.scope.wide {
            self.wide_pages = idx.device_pages(doc);
        } else {
            let verify = ecad_model::document::ledger::verify_mode();
            for c in &self.scope.devices {
                let derived = match reuse.get(c) {
                    // Verify mode checks a reuse against a fresh derivation; a stale one (a bug) is
                    // reported and the fresh one used.
                    Some(kept) if verify => {
                        let fresh = shown(doc, &mut places, *c, m);
                        if *kept != fresh {
                            self.problems.push(format!("the texts kept for {} were stale; derived again", c.hex()));
                        }
                        fresh
                    }
                    Some(kept) => kept.clone(),
                    None => shown(doc, &mut places, *c, m),
                };
                self.before.insert(*c, derived);
            }
        }
        if self.scope.arrows_wide {
            self.arrow_pages = idx.arrow_pages(doc);
        } else {
            self.before_names = crate::continuations::shown_measured(doc, &mut places, &self.scope.names, m);
        }
    }

    /// Bring `idx` up to date for the groups noted on both sides (the caller makes it again whole
    /// instead when [`Watch::rebuilds`]).
    pub(crate) fn refresh(&self, doc: &Document, idx: &mut RefIndex) {
        idx.refresh(doc, &self.groups);
    }

    /// After the After notes: `doc` is the state after the commit and `idx` its index, already
    /// brought up to date. Says what changed. A watch never taken compares nothing.
    pub(crate) fn finish(mut self, doc: &Document, idx: &RefIndex, m: &dyn Look) -> Outcome {
        let Some(cables) = self.cables.take() else { return Outcome::default() };
        if self.rebuild {
            self.scope.wide = true;
        }
        let scope = (!self.all_cables).then_some(&self.cable_scope);
        let after_cables = crate::references::cables::Snapshot::take(doc, scope);
        let orphaned = cables.orphaned_line(doc, &after_cables);
        let mut pages = cables.pages_changed(&after_cables);
        self.scope.resolve(doc, idx, m);
        let mut shown_after = Shown::new();
        let device_pages: BTreeSet<PageId> = if self.scope.wide {
            let mut p = std::mem::take(&mut self.wide_pages);
            p.extend(idx.device_pages(doc));
            p
        } else {
            let mut places = Places::new(doc);
            let mut out = BTreeSet::new();
            for c in &self.scope.devices {
                let old = self.before.remove(c).unwrap_or_default();
                let new = shown(doc, &mut places, *c, m);
                for page in old.keys().chain(new.keys()) {
                    if old.get(page) != new.get(page) {
                        out.insert(*page);
                    }
                }
                shown_after.insert(*c, new);
            }
            out
        };
        if let Some(p) = &mut pages {
            p.extend(device_pages);
            // The pages whose continuation texts or caption runs
            // changed, or every page with an arrow before and after.
            if self.scope.arrows_wide {
                p.extend(std::mem::take(&mut self.arrow_pages));
                p.extend(idx.arrow_pages(doc));
            } else if !self.scope.names.is_empty() {
                let mut places = Places::new(doc);
                let after = crate::continuations::shown_measured(doc, &mut places, &self.scope.names, m);
                for page in self.before_names.keys().chain(after.keys()) {
                    if self.before_names.get(page) != after.get(page) {
                        p.insert(*page);
                    }
                }
            }
        }
        Outcome { pages, orphaned, shown: shown_after, problems: self.problems }
    }
}
