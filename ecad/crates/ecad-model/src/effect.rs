//! What a transaction changed, classified once (design 064 §3): [`Effects::of`].
//!
//! Every derived view (checks, reports, references, the change log's feeds) states its update map
//! against [`Effect`], never against [`Op`]. This module holds the **only** `match` on `Op` those
//! views rely on, with no wildcard arm: a new op does not compile until it is classified here.
//!
//! An effect is computed from the ops alone. Every op carries what it changes (whole objects for
//! add and remove, `before` and `after` for modify), so the classifier needs no document and never
//! fails. A view looks things up in the document itself.
//!
//! This module excludes what an effect means to a view (each view's `note` decides that) and the
//! document lookups.
#![deny(clippy::wildcard_enum_match_arm)]

use crate::doc::{Cable, CableMember, Definition, Device, End, Folder, Item, Layer, Page, ProjectSettings, Representation, Terminal, Wire};
use crate::id::{CableId, DefinitionId, DeviceId, FolderId, ItemId, LayerId, PageId, RepresentationId, TerminalId, WireId};
use crate::keys::{Area, area_of};
use crate::op::{ObjectRef, Op};
use crate::project::Project;
use crate::style::{StandardHeader, StyleLayer, StyleRecord, StyleRef};

/// A property key with its area, classified once ([`area_of`], design 064 §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key<'a> {
    pub raw: &'a str,
    pub area: Area,
}

/// One thing a transaction changed. `before: None` = created, `after: None` = removed.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect<'a> {
    /// A page added, removed, changed in place (`ModifyPage`, which states no index), or removed
    /// and added in one transaction (a move: the indexes differ).
    Page {
        id: PageId,
        before: Option<&'a Page>,
        after: Option<&'a Page>,
        index_before: Option<usize>,
        index_after: Option<usize>,
    },
    Layer {
        id: LayerId,
        before: Option<&'a Layer>,
        after: Option<&'a Layer>,
    },
    Item {
        id: ItemId,
        before: Option<&'a Item>,
        after: Option<&'a Item>,
    },
    Device {
        id: DeviceId,
        before: Option<&'a Device>,
        after: Option<&'a Device>,
    },
    /// A component's definition switched (`None`: no definition).
    DeviceDefinition {
        id: DeviceId,
        before: Option<DefinitionId>,
        after: Option<DefinitionId>,
    },
    /// A connection point added, removed, or both (brief 028's relabel).
    Terminal {
        id: TerminalId,
        before: Option<&'a Terminal>,
        after: Option<&'a Terminal>,
    },
    Rep {
        id: RepresentationId,
        before: Option<&'a Representation>,
        after: Option<&'a Representation>,
    },
    Wire {
        id: WireId,
        before: Option<&'a Wire>,
        after: Option<&'a Wire>,
    },
    /// A conductor end attached or detached.
    End {
        conductor: WireId,
        end: End,
        before: Option<TerminalId>,
        after: Option<TerminalId>,
    },
    Cable {
        id: CableId,
        before: Option<&'a Cable>,
        after: Option<&'a Cable>,
    },
    Member {
        cable: CableId,
        index: usize,
        before: Option<&'a CableMember>,
        after: Option<&'a CableMember>,
    },
    Prop {
        object: ObjectRef,
        key: Key<'a>,
        before: Option<&'a str>,
        after: Option<&'a str>,
    },
    Definition {
        id: DefinitionId,
        before: Option<&'a Definition>,
        after: Option<&'a Definition>,
    },
    Settings {
        before: &'a ProjectSettings,
        after: &'a ProjectSettings,
    },
    /// A version named (`named`) or unnamed at log position `at`.
    Version {
        name: &'a str,
        at: u64,
        named: bool,
    },
    /// A folder added, removed, changed in place, or removed and added again. `index_before` and
    /// `index_after` are the places in the folder order the ops state: an add states the new
    /// place, a remove the old one, and a change in place (`ModifyFolder`) states none.
    Folder {
        id: FolderId,
        before: Option<&'a Folder>,
        after: Option<&'a Folder>,
        index_before: Option<usize>,
        index_after: Option<usize>,
    },
    /// A style record set. `catalogue` is the catalogue's stored word until `SetStyle` carries a
    /// `StyleCatalogue` (lane 072 A2).
    Style {
        layer: StyleLayer,
        catalogue: &'a str,
        id: &'a StyleRef,
        before: Option<&'a StyleRecord>,
        after: Option<&'a StyleRecord>,
    },
    Standard {
        before: Option<&'a StandardHeader>,
        after: Option<&'a StandardHeader>,
    },
    /// The project's own data changed (`ModifyProject`, design 072 §5.3).
    Project {
        before: &'a Project,
        after: &'a Project,
    },
}

/// The kind of an [`Effect`], without its data: for tests, the update-map matrix and cheap "any
/// effect of kind X?" questions ([`EffectKindSet`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EffectKind {
    Page,
    Layer,
    Item,
    Device,
    DeviceDefinition,
    Terminal,
    Rep,
    Wire,
    End,
    Cable,
    Member,
    Prop,
    Definition,
    Settings,
    Version,
    Folder,
    Style,
    Standard,
    Project,
}

impl EffectKind {
    /// Every kind, in declaration order.
    pub const ALL: [EffectKind; 19] = [
        EffectKind::Page,
        EffectKind::Layer,
        EffectKind::Item,
        EffectKind::Device,
        EffectKind::DeviceDefinition,
        EffectKind::Terminal,
        EffectKind::Rep,
        EffectKind::Wire,
        EffectKind::End,
        EffectKind::Cable,
        EffectKind::Member,
        EffectKind::Prop,
        EffectKind::Definition,
        EffectKind::Settings,
        EffectKind::Version,
        EffectKind::Folder,
        EffectKind::Style,
        EffectKind::Standard,
        EffectKind::Project,
    ];

    fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

/// A set of [`EffectKind`]s.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EffectKindSet(u32);

impl EffectKindSet {
    /// Whether `kind` is in the set.
    pub fn contains(self, kind: EffectKind) -> bool {
        self.0 & kind.bit() != 0
    }

    /// Add `kind`.
    pub fn insert(&mut self, kind: EffectKind) {
        self.0 |= kind.bit();
    }

    /// Whether the set is empty.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A set of property [`Area`]s, buildable in a `const` (a view's `AREAS`, design 064 §3.3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AreaSet(u64);

impl AreaSet {
    /// No area.
    pub const NONE: AreaSet = AreaSet(0);
    /// Every area, [`Area::Free`] included.
    pub const ALL: AreaSet = AreaSet(u64::MAX);

    /// The set of `areas`.
    pub const fn of(areas: &[Area]) -> AreaSet {
        let mut bits = 0u64;
        let mut i = 0;
        while i < areas.len() {
            bits |= 1 << (areas[i] as u64);
            i += 1;
        }
        AreaSet(bits)
    }

    /// Whether `area` is in the set.
    pub const fn contains(self, area: Area) -> bool {
        self.0 & (1 << (area as u64)) != 0
    }
}

impl Effect<'_> {
    /// This effect's kind.
    pub fn kind(&self) -> EffectKind {
        match self {
            Effect::Page { .. } => EffectKind::Page,
            Effect::Layer { .. } => EffectKind::Layer,
            Effect::Item { .. } => EffectKind::Item,
            Effect::Device { .. } => EffectKind::Device,
            Effect::DeviceDefinition { .. } => EffectKind::DeviceDefinition,
            Effect::Terminal { .. } => EffectKind::Terminal,
            Effect::Rep { .. } => EffectKind::Rep,
            Effect::Wire { .. } => EffectKind::Wire,
            Effect::End { .. } => EffectKind::End,
            Effect::Cable { .. } => EffectKind::Cable,
            Effect::Member { .. } => EffectKind::Member,
            Effect::Prop { .. } => EffectKind::Prop,
            Effect::Definition { .. } => EffectKind::Definition,
            Effect::Settings { .. } => EffectKind::Settings,
            Effect::Version { .. } => EffectKind::Version,
            Effect::Folder { .. } => EffectKind::Folder,
            Effect::Style { .. } => EffectKind::Style,
            Effect::Standard { .. } => EffectKind::Standard,
            Effect::Project { .. } => EffectKind::Project,
        }
    }
}

/// The effects of a transaction's ops, in op order, with pairs folded.
///
/// # Semantics
/// One effect per op, except that a remove followed by an add of the same page, connection point
/// or folder in one transaction folds into one effect with both sides, standing where the remove
/// stood. A pair separated by another op naming that object (another effect on it, or for a page
/// a property of the page) is not folded.
///
/// # Example
/// ```
/// use ecad_model::effect::{EffectKind, Effects};
/// use ecad_model::{Op, ProjectSettings};
/// let s = ProjectSettings::default();
/// let ops = [Op::SetProjectSettings { before: s.clone(), after: s }];
/// let e = Effects::of(&ops);
/// assert!(e.kinds().contains(EffectKind::Settings));
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Effects<'a> {
    list: Vec<Effect<'a>>,
}

/// The object a fold waits for an add of: which kind, and the id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    Page(PageId),
    Terminal(TerminalId),
    Folder(FolderId),
}

impl<'a> Effects<'a> {
    /// Classify `ops`.
    pub fn of(ops: &'a [Op]) -> Self {
        let mut list: Vec<Effect<'a>> = Vec::with_capacity(ops.len());
        // Removes waiting for an add of the same object: (object, index in `list`).
        let mut open: Vec<(Open, usize)> = Vec::new();
        for op in ops {
            let e = classify(op);
            // An add that completes a waiting remove fills its `after` side.
            if let Some(k) = fold_key(&e, false)
                && let Some(at) = open.iter().position(|(o, _)| *o == k)
            {
                let (_, i) = open.swap_remove(at);
                if let Some(slot) = list.get_mut(i) {
                    fill_after(slot, e);
                }
                continue;
            }
            // Any other effect naming a waiting object ends the wait.
            open.retain(|(o, _)| !names(&e, *o));
            if let Some(k) = fold_key(&e, true) {
                open.push((k, list.len()));
            }
            list.push(e);
        }
        Effects { list }
    }

    /// The effects, in op order.
    pub fn iter(&self) -> std::slice::Iter<'_, Effect<'a>> {
        self.list.iter()
    }

    /// How many effects there are.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The kinds present.
    pub fn kinds(&self) -> EffectKindSet {
        let mut s = EffectKindSet::default();
        for e in &self.list {
            s.insert(e.kind());
        }
        s
    }
}

impl<'e, 'a> IntoIterator for &'e Effects<'a> {
    type Item = &'e Effect<'a>;
    type IntoIter = std::slice::Iter<'e, Effect<'a>>;
    fn into_iter(self) -> Self::IntoIter {
        self.list.iter()
    }
}

/// The one classification of an op.
fn classify(op: &Op) -> Effect<'_> {
    match op {
        Op::AddPage { id, page, index } => Effect::Page { id: *id, before: None, after: Some(page), index_before: None, index_after: Some(*index) },
        Op::RemovePage { id, page, index } => {
            Effect::Page { id: *id, before: Some(page), after: None, index_before: Some(*index), index_after: None }
        }
        Op::ModifyPage { id, before, after } => {
            Effect::Page { id: *id, before: Some(before), after: Some(after), index_before: None, index_after: None }
        }
        Op::AddLayer { id, layer } => Effect::Layer { id: *id, before: None, after: Some(layer) },
        Op::RemoveLayer { id, layer } => Effect::Layer { id: *id, before: Some(layer), after: None },
        Op::AddItem { id, item } => Effect::Item { id: *id, before: None, after: Some(item) },
        Op::ModifyItem { id, before, after } => Effect::Item { id: *id, before: Some(before), after: Some(after) },
        Op::RemoveItem { id, item } => Effect::Item { id: *id, before: Some(item), after: None },
        Op::CreateDevice { id, device } => Effect::Device { id: *id, before: None, after: Some(device) },
        Op::DeleteDevice { id, device } => Effect::Device { id: *id, before: Some(device), after: None },
        Op::ModifyDevice { id, before, after } => Effect::Device { id: *id, before: Some(before), after: Some(after) },
        Op::SetDeviceDefinition { id, before, after } => Effect::DeviceDefinition { id: *id, before: *before, after: *after },
        Op::AddTerminal { id, terminal } => Effect::Terminal { id: *id, before: None, after: Some(terminal) },
        Op::RemoveTerminal { id, terminal } => Effect::Terminal { id: *id, before: Some(terminal), after: None },
        Op::LinkRepresentation { id, rep } => Effect::Rep { id: *id, before: None, after: Some(rep) },
        Op::ModifyRepresentation { id, before, after } => Effect::Rep { id: *id, before: Some(before), after: Some(after) },
        Op::UnlinkRepresentation { id, rep } => Effect::Rep { id: *id, before: Some(rep), after: None },
        Op::CreateWire { id, conductor } => Effect::Wire { id: *id, before: None, after: Some(conductor) },
        Op::DeleteWire { id, conductor } => Effect::Wire { id: *id, before: Some(conductor), after: None },
        Op::ModifyWire { id, before, after } => Effect::Wire { id: *id, before: Some(before), after: Some(after) },
        Op::AttachEnd { conductor, end, before, after } => Effect::End { conductor: *conductor, end: *end, before: *before, after: *after },
        Op::CreateCable { id, cable } => Effect::Cable { id: *id, before: None, after: Some(cable) },
        Op::DeleteCable { id, cable } => Effect::Cable { id: *id, before: Some(cable), after: None },
        Op::AddCableMember { cable, index, member } => Effect::Member { cable: *cable, index: *index, before: None, after: Some(member) },
        Op::RemoveCableMember { cable, index, member } => Effect::Member { cable: *cable, index: *index, before: Some(member), after: None },
        Op::SetProperty { object, key, before, after } => {
            Effect::Prop { object: *object, key: Key { raw: key, area: area_of(key) }, before: before.as_deref(), after: after.as_deref() }
        }
        Op::EmbedDefinition { id, definition } => Effect::Definition { id: *id, before: None, after: Some(definition) },
        Op::RemoveDefinition { id, definition } => Effect::Definition { id: *id, before: Some(definition), after: None },
        Op::SetProjectSettings { before, after } => Effect::Settings { before, after },
        Op::NameVersion { name, at, .. } => Effect::Version { name, at: *at, named: true },
        Op::UnnameVersion { name, at, .. } => Effect::Version { name, at: *at, named: false },
        Op::AddFolder { id, folder, index } => {
            Effect::Folder { id: *id, before: None, after: Some(folder), index_before: None, index_after: Some(*index) }
        }
        Op::RemoveFolder { id, folder, index } => {
            Effect::Folder { id: *id, before: Some(folder), after: None, index_before: Some(*index), index_after: None }
        }
        Op::ModifyFolder { id, before, after } => {
            Effect::Folder { id: *id, before: Some(before), after: Some(after), index_before: None, index_after: None }
        }
        Op::SetStyle { layer, catalogue, id, before, after } => {
            Effect::Style { layer: *layer, catalogue, id, before: before.as_ref(), after: after.as_ref() }
        }
        Op::SetStandard { before, after } => Effect::Standard { before: before.as_ref(), after: after.as_ref() },
        Op::ModifyProject { before, after } => Effect::Project { before, after },
    }
}

/// The object a removing (`remove`) or adding (`!remove`) effect may fold on, if it is one.
fn fold_key(e: &Effect<'_>, remove: bool) -> Option<Open> {
    match e {
        Effect::Page { id, before: Some(_), after: None, .. } if remove => Some(Open::Page(*id)),
        Effect::Page { id, before: None, after: Some(_), .. } if !remove => Some(Open::Page(*id)),
        Effect::Terminal { id, before: Some(_), after: None } if remove => Some(Open::Terminal(*id)),
        Effect::Terminal { id, before: None, after: Some(_) } if !remove => Some(Open::Terminal(*id)),
        Effect::Folder { id, before: Some(_), after: None, .. } if remove => Some(Open::Folder(*id)),
        Effect::Folder { id, before: None, after: Some(_), .. } if !remove => Some(Open::Folder(*id)),
        Effect::Page { .. }
        | Effect::Layer { .. }
        | Effect::Item { .. }
        | Effect::Device { .. }
        | Effect::DeviceDefinition { .. }
        | Effect::Terminal { .. }
        | Effect::Rep { .. }
        | Effect::Wire { .. }
        | Effect::End { .. }
        | Effect::Cable { .. }
        | Effect::Member { .. }
        | Effect::Prop { .. }
        | Effect::Definition { .. }
        | Effect::Settings { .. }
        | Effect::Version { .. }
        | Effect::Folder { .. }
        | Effect::Style { .. }
        | Effect::Standard { .. }
        | Effect::Project { .. } => None,
    }
}

/// Whether `e` names the waiting object `o` (so a later add must not fold over it).
fn names(e: &Effect<'_>, o: Open) -> bool {
    match (e, o) {
        (Effect::Page { id, .. }, Open::Page(p)) => *id == p,
        (Effect::Prop { object: ObjectRef::Page(id), .. }, Open::Page(p)) => *id == p,
        (Effect::Terminal { id, .. }, Open::Terminal(t)) => *id == t,
        (Effect::Folder { id, .. }, Open::Folder(f)) => *id == f,
        _ => false,
    }
}

/// Fill the `after` side of a waiting remove with the add `add`.
fn fill_after<'a>(slot: &mut Effect<'a>, add: Effect<'a>) {
    match (slot, add) {
        (Effect::Page { after, index_after, .. }, Effect::Page { after: a, index_after: i, .. }) => {
            *after = a;
            *index_after = i;
        }
        (Effect::Terminal { after, .. }, Effect::Terminal { after: a, .. }) => *after = a,
        (Effect::Folder { after, index_after, .. }, Effect::Folder { after: a, index_after: i, .. }) => {
            *after = a;
            *index_after = i;
        }
        _ => {}
    }
}
