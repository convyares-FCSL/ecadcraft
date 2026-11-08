//! The ECAD project as a CADCraft document owner.
//!
//! [`Project`] holds the authoritative [`Ledger`] and one view per page. A page that is open in the
//! engine is a CADCraft document whose drawing is a projection of the ECAD document; the
//! [`HandleMap`] relates its handles to ECAD IDs and lives only in the session (proposal §13).

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, Weak};

use cadcraft_engine::doc::{
    Block, Common, Drawing, Entity, EntityKind, EntityStore, Group, Handle, Layer as CLayer, Lineweight, Text,
};
use cadcraft_engine::ext::{Commit, DocumentOwner, OwnerRef, Start, Verdict};
use cadcraft_engine::{EngineError, Session};

use crate::access::{Pages, PagesRef, ProjectHandle};
use ecad_file::{OpenReport, ProjectFile, SaveOutcome};
use ecad_inspect::{EventKind, EventLog, LiveFacts, PageInfo};
use ecad_model::{
    Author, CableId, ConductorId, Document, End, Form, IdAllocator, ItemKind, Layer, LayerId, Ledger, LengthUnit, Op,
    Page, PageId, ProjectId, ProjectSettings, Pt, RepresentationId, Subject, TerminalId,
};
use ecad_store::MemoryMedium;
use serde_json::{Value, json};

use crate::convert::{ECAD_PREFIX, MARKER_LAYER, block_name, entity_for, kind_for_primitive, marker_entity};
use crate::translate;
use crate::verify::verify_page_with;
pub use crate::views::View;

/// What a projected handle stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mapped {
    Item(ecad_model::ItemId),
    /// A terminal marker of a drawn representation.
    Marker(RepresentationId, TerminalId),
    /// A line of a cable's fan view (brief 005): computed, not an item. Edits of it are refused.
    Fan(CableId),
    /// The derived text of a cable reference (brief 010), by its reference representation:
    /// computed, not an item. Erasing it erases its reference; other edits are refused.
    RefText(RepresentationId),
}

/// ECAD ID ⇄ CADCraft handle for one page. Session-only, never saved.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HandleMap {
    by_handle: HashMap<Handle, Mapped>,
    items: BTreeMap<ecad_model::ItemId, Handle>,
    markers: BTreeMap<(RepresentationId, TerminalId), Handle>,
    /// Computed cable drawing (fans, core labels, reference flags; briefs 005, 009): (cable,
    /// index of the part) → handle.
    fans: BTreeMap<(CableId, u32), Handle>,
    /// The cables drawn as fans on this page.
    fan_cables: BTreeSet<CableId>,
    /// The member and the reference end each computed part belongs to, where it has one.
    parts: HashMap<Handle, (Option<ConductorId>, Option<RepresentationId>)>,
    /// Loose wire ends at a cable end that stops at a reference (design 009 §7.3): no loose-end
    /// square is shown there.
    quiet: Vec<Pt>,
    /// Reference texts (brief 010): reference representation → handle.
    refs: BTreeMap<RepresentationId, Handle>,
}

impl HandleMap {
    pub fn get(&self, h: Handle) -> Option<Mapped> {
        self.by_handle.get(&h).copied()
    }
    pub fn item(&self, id: ecad_model::ItemId) -> Option<Handle> {
        self.items.get(&id).copied()
    }
    pub fn marker(&self, rep: RepresentationId, t: TerminalId) -> Option<Handle> {
        self.markers.get(&(rep, t)).copied()
    }
    pub fn insert_item(&mut self, id: ecad_model::ItemId, h: Handle) {
        self.items.insert(id, h);
        self.by_handle.insert(h, Mapped::Item(id));
    }
    pub fn insert_marker(&mut self, rep: RepresentationId, t: TerminalId, h: Handle) {
        self.markers.insert((rep, t), h);
        self.by_handle.insert(h, Mapped::Marker(rep, t));
    }
    pub fn fan(&self, cable: CableId, index: u32) -> Option<Handle> {
        self.fans.get(&(cable, index)).copied()
    }
    pub fn insert_fan(&mut self, cable: CableId, index: u32, h: Handle) {
        self.fans.insert((cable, index), h);
        self.by_handle.insert(h, Mapped::Fan(cable));
    }
    /// A computed part of `cable`'s drawing (brief 009): `fan` when the cable is drawn as a fan on
    /// this page; the member (`core`) and the reference end it belongs to.
    pub fn insert_part(
        &mut self,
        cable: CableId,
        index: u32,
        h: Handle,
        fan: bool,
        core: Option<ConductorId>,
        reference: Option<RepresentationId>,
    ) {
        self.insert_fan(cable, index, h);
        if fan {
            self.fan_cables.insert(cable);
        }
        if core.is_some() || reference.is_some() {
            self.parts.insert(h, (core, reference));
        }
    }
    /// Loose wire ends that show no loose-end square (design 009 §7.3).
    pub fn quiet_ends(&self) -> &[Pt] {
        &self.quiet
    }
    /// Whether `cable` is drawn as a fan on this page.
    pub fn is_fan(&self, cable: CableId) -> bool {
        self.fan_cables.contains(&cable)
    }
    /// The member a computed part belongs to (a bend, a tail, a core label, a wire number).
    pub fn core_of(&self, h: Handle) -> Option<ConductorId> {
        self.parts.get(&h).and_then(|(c, _)| *c)
    }
    /// The reference end a computed part belongs to (its flag or text).
    pub fn reference_of(&self, h: Handle) -> Option<RepresentationId> {
        self.parts.get(&h).and_then(|(_, r)| *r)
    }
    /// Every handle of a fan view line.
    pub fn fan_handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.fans.values().copied()
    }
    /// The handle of a reference's derived text.
    pub fn ref_text(&self, reference: RepresentationId) -> Option<Handle> {
        self.refs.get(&reference).copied()
    }
    pub fn insert_ref_text(&mut self, reference: RepresentationId, h: Handle) {
        self.refs.insert(reference, h);
        self.by_handle.insert(h, Mapped::RefText(reference));
    }
    /// Every handle of a reference text.
    pub fn ref_text_handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.refs.values().copied()
    }
    pub fn len(&self) -> usize {
        self.by_handle.len()
    }
    pub fn is_empty(&self) -> bool {
        self.by_handle.is_empty()
    }
    /// Every mapped handle.
    pub fn handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.by_handle.keys().copied()
    }
    /// Whether the map is one-to-one in both directions.
    pub fn is_consistent(&self) -> bool {
        self.by_handle.len() == self.items.len() + self.markers.len() + self.fans.len() + self.refs.len()
            && self.items.iter().all(|(id, h)| self.by_handle.get(h) == Some(&Mapped::Item(*id)))
            && self.markers.iter().all(|((r, t), h)| self.by_handle.get(h) == Some(&Mapped::Marker(*r, *t)))
            && self.fans.iter().all(|((k, _), h)| self.by_handle.get(h) == Some(&Mapped::Fan(*k)))
            && self.refs.iter().all(|(r, h)| self.by_handle.get(h) == Some(&Mapped::RefText(*r)))
    }
}

/// A page and, when it is open, the engine document showing it.
#[derive(Debug, Clone, Default)]
pub struct PageView {
    pub uid: Option<u64>,
    pub map: HandleMap,
    /// The drawing the owner last wrote (or accepted) for this page.
    pub last: Option<Arc<Drawing>>,
    /// Projection and rebuild both failed: the page must be closed and reopened (PROJ-02).
    pub unavailable: bool,
    /// A commit on another page changed what this page shows (its reference texts, brief 010),
    /// and the page was not redrawn then: only the page being shown is redrawn on a commit (owner,
    /// 2026-11-08). It is redrawn when it comes to the front ([`ProjectHandle::refresh_shown`]).
    pub stale: bool,
}

/// Injected failures for the experiment (never set in normal use).
#[derive(Debug, Clone, Default)]
pub struct Faults {
    /// The next `n` projection attempts fail.
    pub fail_projections: u32,
    /// The next `n` cable view switches (`cableview` with a cable) fail before changing anything.
    pub fail_view_switches: u32,
    /// The next commit callback panics (`fault-injection` feature only).
    #[cfg(feature = "fault-injection")]
    pub panic_in_commit: bool,
    /// The next ECAD command panics before committing (`fault-injection` feature only).
    #[cfg(feature = "fault-injection")]
    pub panic_in_command: bool,
    /// While set, the prompts and previews of the interactive forms panic (`fault-injection`
    /// feature only).
    #[cfg(feature = "fault-injection")]
    pub panic_in_form_preview: bool,
}

/// Whether a project can be saved, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persistence {
    /// Automated tests only: saving writes the real `.ecad` format (`ecad-file`) to an
    /// in-memory medium and marks the project clean. No file is written to disk.
    TestStore,
    /// No persistence: Save and Save As report that saving is unavailable, change nothing and
    /// never mark the project clean.
    Unavailable,
    /// Project files on disk (`.ecad`, persistence stage 2a): Save writes to the bound file,
    /// Save As (and the first save) writes a new one; see [`crate::file`].
    File,
}

/// The test-only project file: the real `.ecad` format (`ecad-file`) over an in-memory medium
/// (persistence stage 1). It is written on the first save; until then it is an empty medium.
#[derive(Debug)]
pub enum MemoryStore {
    /// Not saved yet: an empty medium (tests may set its fault flags before the first save).
    Unwritten(MemoryMedium),
    /// Written: the file and its writer state.
    Written(Box<ProjectFile<MemoryMedium>>),
}

impl Default for MemoryStore {
    fn default() -> Self {
        MemoryStore::Unwritten(MemoryMedium::default())
    }
}

impl MemoryStore {
    /// The medium holding the file's bytes.
    pub fn medium(&self) -> &MemoryMedium {
        match self {
            MemoryStore::Unwritten(m) => m,
            MemoryStore::Written(f) => f.medium(),
        }
    }

    /// The medium, to inject faults in tests.
    pub fn medium_mut(&mut self) -> &mut MemoryMedium {
        match self {
            MemoryStore::Unwritten(m) => m,
            MemoryStore::Written(f) => f.medium_mut(),
        }
    }

    /// The writer, once the file exists.
    pub fn file(&self) -> Option<&ProjectFile<MemoryMedium>> {
        match self {
            MemoryStore::Unwritten(_) => None,
            MemoryStore::Written(f) => Some(&**f),
        }
    }

    /// Save: the first save writes the whole history (Save As semantics: a failure leaves no
    /// file); later saves append with retry (§20.5). Returns the persisted `seq`.
    fn save(&mut self, project: ProjectId, ledger: &mut Ledger, writer: &str) -> Result<u64, String> {
        match self {
            MemoryStore::Written(f) => {
                f.save(ledger).map_err(|e| e.to_string()).map(|_: SaveOutcome| ledger.persisted())
            }
            MemoryStore::Unwritten(m) => {
                let medium = std::mem::take(m);
                let f = ProjectFile::create(medium, project, ledger, now_ms(), writer).map_err(|e| e.to_string())?;
                *self = MemoryStore::Written(Box::new(f));
                Ok(ledger.persisted())
            }
        }
    }
}

/// What Save and Save As report when the project has no persistence.
pub const SAVE_UNAVAILABLE: &str = "Saving is not available in the ECAD trial: project persistence is not implemented. \
Nothing was written and the project is still unsaved. Closing it discards the design.";

/// A copy of the project's meaning taken when objects were put on the clipboard.
#[derive(Debug, Clone)]
pub struct ClipSnapshot {
    pub doc: Document,
    pub page: PageId,
    pub map: HandleMap,
}

/// An ECAD project attached to a CADCraft session.
pub struct Project {
    pub name: String,
    /// The project's file path once it has one; every page document carries it, so the engine's
    /// Save goes to the owner and a never-saved project gets Save As.
    pub path: Option<String>,
    pub ledger: Ledger,
    pub ids: IdAllocator,
    /// The project's stable identity (the file header records it).
    pub id: ProjectId,
    /// The test-only in-memory project file (`Persistence::TestStore`).
    pub store: MemoryStore,
    /// The project's file on disk (`Persistence::File`).
    pub file: crate::file::Binding,
    /// What opening reported, when the project was opened from a file.
    pub open_report: Option<OpenReport>,
    pub pages: BTreeMap<PageId, PageView>,
    pub events: EventLog,
    pub clips: BTreeMap<u64, ClipSnapshot>,
    pub faults: Faults,
    /// How Save behaves; see [`Persistence`].
    pub persistence: Persistence,
    /// The head right after the project was created or opened: with no persistence, the project
    /// has unsaved design exactly when the head has moved from here.
    baseline: u64,
    /// Distance within which a point lands on a terminal.
    pub tolerance: f64,
    /// Layer used for new ECAD-drawn geometry.
    pub default_layer: LayerId,
    me: Weak<Mutex<Project>>,
    /// Points `DocState::saved` away from the drawing while the project is unsaved.
    unsaved_marker: Arc<Drawing>,
    /// Cables shown in fan view (brief 005); every other cable is traditional. Presentation only:
    /// never in the ledger, never a transaction, never the unsaved state (P5, SAVE-03).
    pub views: BTreeSet<CableId>,
    /// Where this user's cable views are remembered ([`crate::views::Settings`]).
    pub settings: crate::views::Settings,
    /// Whether [`crate::views::NOT_REMEMBERED`] was said in this session.
    pub(crate) said_not_remembered: bool,
    /// Who the project's edits, undos and redos are recorded as (FILE-07): [`Author::User`] by
    /// default, `Author::Agent(<client name>)` when an agent drives the session (brief 011). A
    /// session setting, not part of the design: set it through [`ProjectHandle::configure`].
    pub author: Author,
}

/// What this program records about itself in a file header and a writer lock.
pub(crate) fn writer() -> String {
    format!("ecad {} (cadcraft {})", env!("CARGO_PKG_VERSION"), crate::UPSTREAM_COMMIT.get(..7).unwrap_or_default())
}

/// A project's name from its file path: the file name without `.ecad`.
fn file_stem(path: &str) -> String {
    std::path::Path::new(path).file_stem().map_or_else(|| path.to_string(), |s| s.to_string_lossy().into_owned())
}

/// Milliseconds since the epoch, where a clock exists.
pub(crate) fn now_ms() -> Option<u64> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// A seed for IDs that differs for every call: a per-process counter mixed with the clock and,
/// natively, the process id. Two projects created in the same millisecond, or on wasm (no clock),
/// still get different ID streams.
pub fn session_seed() -> u64 {
    static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(target_arch = "wasm32"))]
    let pid = u64::from(std::process::id());
    #[cfg(target_arch = "wasm32")]
    let pid = 0_u64;
    // SplitMix64 finaliser over the combined inputs, so nearby values give unrelated seeds.
    let mut z = now_ms().unwrap_or(0x5EED) ^ pid.rotate_left(40) ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The reserved layer of cable fan views (`ecad.cable`, plots).
fn cables_layer() -> CLayer {
    let mut l = CLayer::new(crate::views::LAYER);
    l.color = cadcraft_engine::doc::color::Color::Index(crate::views::CABLE_ACI);
    l
}

/// One computed part of a cable's drawing (briefs 005, 009) as an entity on the reserved layer:
/// a line for two points, else a polyline; an arc; a text. Trunk and spine carry the cable
/// lineweight, bends, tails and frames the wire lineweight, text the layer's.
pub(crate) fn part_entity(part: &crate::look::Part, handle: Handle) -> Entity {
    use crate::look::{LABEL_HEIGHT, Shape, Weight};
    use ecad_model::Primitive;
    let kind = match &part.shape {
        Shape::Path { points, closed: false } if points.len() == 2 => match points.as_slice() {
            [a, b] => kind_for_primitive(&Primitive::Line { a: *a, b: *b }),
            _ => kind_for_primitive(&Primitive::Polyline { vertices: points.clone(), closed: false }),
        },
        Shape::Path { points, closed } => {
            kind_for_primitive(&Primitive::Polyline { vertices: points.clone(), closed: *closed })
        }
        Shape::Arc { center, radius, start, end } => {
            kind_for_primitive(&Primitive::Arc { center: *center, radius: *radius, start: *start, end: *end })
        }
        Shape::Text { at, rotation, value } => kind_for_primitive(&Primitive::Text {
            at: *at,
            height: LABEL_HEIGHT,
            rotation: *rotation,
            value: value.clone(),
        }),
    };
    let lineweight = match part.weight {
        Weight::Cable => Lineweight::Mm100(crate::look::CABLE_WEIGHT),
        Weight::Wire => Lineweight::Mm100(crate::look::WIRE_WEIGHT),
        Weight::Text => Lineweight::ByLayer,
    };
    let color =
        part.color.map_or(cadcraft_engine::doc::color::Color::ByLayer, cadcraft_engine::doc::color::Color::Index);
    Entity {
        handle,
        common: Common { layer: crate::views::LAYER.into(), lineweight, color, ..Common::default() },
        kind,
    }
}

fn base_drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    if d.layer(MARKER_LAYER).is_none() {
        let mut l = CLayer::new(MARKER_LAYER);
        l.plot = false;
        l.color = cadcraft_engine::doc::color::Color::Index(8);
        d.layers.push(l);
    }
    d
}

/// The label an unavailable page shows instead of its drawing.
pub const PLACEHOLDER_TEXT: &str = "PAGE UNAVAILABLE: its drawing could not be rebuilt. The project is intact and can be saved. Close this page and reopen it.";
/// Appended to an unavailable page's title.
pub const UNAVAILABLE_SUFFIX: &str = " [unavailable]";

/// The drawing an unavailable page shows: one label, not editable (every change is refused).
fn placeholder_drawing() -> Drawing {
    let mut d = base_drawing();
    let handle = d.new_handle();
    let text = Text {
        insert: cadcraft_engine::geom::Vec3::new(0.0, 0.0, 0.0),
        align_pt: None,
        height: 5.0,
        value: PLACEHOLDER_TEXT.into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: Default::default(),
        valign: Default::default(),
    };
    let mut store = EntityStore::new();
    store.push(Entity {
        handle,
        common: Common { layer: "0".into(), ..Common::default() },
        kind: EntityKind::Text(text),
    });
    d.model = store;
    d
}

/// Project one page of `doc` onto `base` (a drawing whose tables and header are kept), reusing
/// handles from `prev` for objects that are still there.
///
/// # Errors
/// An item that cannot be drawn (missing layer or definition).
pub fn project_page(
    doc: &Document,
    page: PageId,
    base: &Drawing,
    prev: &HandleMap,
) -> Result<(Drawing, HandleMap), String> {
    project_page_with(doc, page, base, prev, &BTreeSet::new(), 0.0)
}

/// [`project_page`] with cable views (brief 005): each cable in `fan` whose fan can be drawn on
/// the page is shown as its fan ([`crate::views::fan_on_page`], with tolerance `tol`) on layer
/// [`crate::views::LAYER`] instead of its member routes and slashes. Everything else is projected
/// exactly as by [`project_page`]. Both draw each cable reference's derived text on that layer
/// too ([`crate::references::markers_on_page`], brief 010).
///
/// # Errors
/// As [`project_page`].
pub fn project_page_with(
    doc: &Document,
    page: PageId,
    base: &Drawing,
    prev: &HandleMap,
    fan: &BTreeSet<CableId>,
    tol: f64,
) -> Result<(Drawing, HandleMap), String> {
    let mut d = base.clone();
    for l in doc.layers.values() {
        if d.layer(&l.name).is_none() {
            d.layers.push(CLayer::new(&l.name));
        }
    }
    if d.layer(MARKER_LAYER).is_none() {
        let mut l = CLayer::new(MARKER_LAYER);
        l.plot = false;
        d.layers.push(l);
    }
    if d.layer(crate::views::LAYER).is_none() {
        d.layers.push(cables_layer());
    }
    for h in prev.handles() {
        d.bump_handseed(h);
    }
    // Blocks for the definitions shown on this page.
    let used: BTreeSet<_> = doc
        .items
        .values()
        .filter(|i| i.page == page)
        .filter_map(|i| match &i.kind {
            ItemKind::Symbol(s) => Some(s.definition),
            ItemKind::Primitive(_) => None,
        })
        .collect();
    d.blocks.retain(|name, _| {
        !name.starts_with(ECAD_PREFIX) || crate::convert::definition_of_block(name).is_some_and(|id| used.contains(&id))
    });
    for def_id in &used {
        let name = block_name(*def_id);
        if d.blocks.contains_key(&name) {
            continue;
        }
        let def = doc.definitions.get(def_id).ok_or_else(|| format!("definition {def_id} is missing"))?;
        let mut b = Block::new(&name);
        for g in &def.graphics {
            let h = d.new_handle();
            let common = cadcraft_engine::doc::Common {
                layer: "0".into(),
                color: cadcraft_engine::doc::color::Color::ByBlock,
                ..Default::default()
            };
            b.entities.push(Entity { handle: h, common, kind: kind_for_primitive(g) });
        }
        for t in &def.terminals {
            let h = d.new_handle();
            b.entities.push(marker_entity(t.at, h));
        }
        d.blocks.insert(name, Arc::new(b));
    }
    let fans = crate::views::drawings_on_page(doc, page, fan, tol);
    let hidden: BTreeSet<ecad_model::ItemId> = fans.values().flat_map(|f| f.hidden.iter().copied()).collect();
    let mut map = HandleMap::default();
    let mut ents: Vec<Entity> = Vec::new();
    for (id, item) in doc.items.iter().filter(|(id, i)| i.page == page && !hidden.contains(id)) {
        let h = prev.item(*id).unwrap_or_else(|| d.new_handle());
        let e = entity_for(doc, item, h).ok_or_else(|| format!("item {id} cannot be drawn"))?;
        ents.push(e);
        map.insert_item(*id, h);
    }
    let mut groups = Vec::new();
    for (rid, r) in doc.representations.iter().filter(|(_, r)| r.page == page && r.form == Form::Drawn) {
        let mut members: Vec<Handle> = r.items.iter().filter_map(|i| map.item(*i)).collect();
        for (t, at) in &r.connection_points {
            let h = prev.marker(*rid, *t).unwrap_or_else(|| d.new_handle());
            ents.push(marker_entity(*at, h));
            map.insert_marker(*rid, *t, h);
            members.push(h);
        }
        if matches!(r.subject, Subject::Component(_)) && members.len() > 1 {
            groups.push(Group {
                name: format!("{ECAD_PREFIX}{:032x}", rid.0),
                description: String::new(),
                selectable: true,
                members,
            });
        }
    }
    for (k, f) in &fans {
        map.quiet.extend(f.quiet.iter().copied());
        for (i, part) in f.parts.iter().enumerate() {
            // A reference end's text (brief 009's look, brief 010's live text) is mapped to its
            // reference, so hover, Go to Reference and erasing it act on the reference.
            if part.role == crate::look::Role::ReferenceText
                && let Some(r) = part.reference
            {
                let h = prev.ref_text(r).unwrap_or_else(|| d.new_handle());
                ents.push(part_entity(part, h));
                map.insert_ref_text(r, h);
                continue;
            }
            let index = u32::try_from(i).map_err(|_| "a cable drawing has too many parts")?;
            let h = prev.fan(*k, index).unwrap_or_else(|| d.new_handle());
            ents.push(part_entity(part, h));
            map.insert_part(*k, index, h, f.is_fan(), part.core, part.reference);
        }
    }
    ents.sort_by_key(|e| e.handle);
    let mut store = EntityStore::new();
    for e in ents {
        store.push(e);
    }
    d.model = store;
    d.groups.retain(|g| !g.name.starts_with(ECAD_PREFIX));
    d.groups.extend(groups);
    Ok((d, map))
}

impl Project {
    /// Create a project with one page, open that page in `s`, and attach the project as its owner.
    ///
    /// # Errors
    /// Projection failure (not expected for an empty project).
    pub fn create(s: &mut Session, name: &str, seed: u64) -> Result<ProjectHandle, String> {
        Self::create_with(s, name, seed, Persistence::TestStore)
    }

    /// [`Project::create`] with a chosen [`Persistence`].
    ///
    /// # Errors
    /// Projection failure (not expected for an empty project).
    pub fn create_with(
        s: &mut Session,
        name: &str,
        seed: u64,
        persistence: Persistence,
    ) -> Result<ProjectHandle, String> {
        let mut ids = IdAllocator::seeded(seed);
        let mut ledger = Ledger::new(seed.rotate_left(32));
        let (page, layer): (PageId, LayerId) = (ids.fresh(), ids.fresh());
        ledger
            .commit_at(
                Author::User,
                "New project",
                vec![
                    Op::AddPage { id: page, page: Page { title: "1".into() }, index: 0 },
                    Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
                    // The length unit is recorded at creation (proposal §20.13).
                    Op::SetProjectSettings {
                        before: ProjectSettings::default(),
                        after: ProjectSettings { length_unit: LengthUnit::Millimetre },
                    },
                ],
                now_ms(),
            )
            .map_err(|e| e.to_string())?;
        let baseline = ledger.head();
        let id: ProjectId = ids.fresh();
        let project = Arc::new(Mutex::new(Project {
            name: name.to_string(),
            path: None,
            ledger,
            ids,
            id,
            store: MemoryStore::default(),
            file: crate::file::Binding::default(),
            open_report: None,
            pages: BTreeMap::new(),
            events: EventLog::default(),
            clips: BTreeMap::new(),
            faults: Faults::default(),
            persistence,
            baseline,
            tolerance: 1e-6,
            default_layer: layer,
            me: Weak::new(),
            unsaved_marker: Arc::new(Drawing::new_metric()),
            views: BTreeSet::new(),
            settings: crate::views::Settings::none(),
            said_not_remembered: false,
            author: Author::User,
        }));
        let mut p = crate::access::try_lock(&project)?;
        p.me = Arc::downgrade(&project);
        // Automated tests (the test store) never touch the user's settings unless they choose a
        // folder; a new project has no remembered views to read.
        if persistence != Persistence::TestStore {
            p.settings = crate::views::Settings::from_env();
        }
        p.pages.insert(page, PageView::default());
        p.open_page(&mut Pages(s), page)?;
        drop(p);
        Ok(ProjectHandle::new(project))
    }

    /// Reopen a project from an in-memory project file (the log is replayed; FILE-02). A damaged
    /// file opens its verified prefix read-only; the report is kept in [`Project::open_report`].
    ///
    /// # Errors
    /// The file has no valid, supported header, or the project cannot be shown.
    pub(crate) fn open_from(
        s: &mut Session,
        name: &str,
        medium: MemoryMedium,
        seed: u64,
    ) -> Result<ProjectHandle, String> {
        let (file, ledger, report) = ProjectFile::open(medium, seed).map_err(|e| e.to_string())?;
        let id = file.header().project_id.ok_or("the file has no project ID")?;
        let default_layer =
            ledger.document().layers.iter().find(|(_, l)| l.name == "0").map(|(id, _)| *id).ok_or("no layer 0")?;
        let pages: Vec<PageId> = ledger.document().page_order.clone();
        // A fresh ID stream even when the caller reuses the creation seed.
        let ids =
            IdAllocator::seeded_after(seed, ledger.log().last().map_or(0, |t| t.id.0) ^ u128::from(ledger.head()));
        let baseline = ledger.head();
        let project = Arc::new(Mutex::new(Project {
            name: name.to_string(),
            path: None,
            ledger,
            ids,
            id,
            store: MemoryStore::Written(Box::new(file)),
            file: crate::file::Binding::default(),
            open_report: Some(report),
            pages: pages.iter().map(|p| (*p, PageView::default())).collect(),
            events: EventLog::default(),
            clips: BTreeMap::new(),
            faults: Faults::default(),
            persistence: Persistence::TestStore,
            baseline,
            tolerance: 1e-6,
            default_layer,
            me: Weak::new(),
            unsaved_marker: Arc::new(Drawing::new_metric()),
            views: BTreeSet::new(),
            settings: crate::views::Settings::none(),
            said_not_remembered: false,
            author: Author::User,
        }));
        let mut p = crate::access::try_lock(&project)?;
        p.me = Arc::downgrade(&project);
        if let Some(first) = pages.first() {
            p.open_page(&mut Pages(s), *first)?;
        }
        drop(p);
        Ok(ProjectHandle::new(project))
    }

    /// Open a project file from disk (`Persistence::File`): take its writer lock and replay it
    /// (FILE-02). A damaged file opens its verified prefix read-only; a file another session is
    /// editing opens read-only. Either can be saved as another file. Returns the handle and what
    /// opening found, in words (also recorded as a [`EventKind::ProjectOpened`] event).
    ///
    /// # Errors
    /// The file cannot be read, or is not a project file this build supports.
    pub fn open_file(s: &mut Session, path: &str, seed: u64) -> Result<(ProjectHandle, String), String> {
        Self::open_file_in(s, path, seed, crate::views::Settings::from_env())
    }

    /// [`Project::open_file`], remembering cable views in `settings` instead of this user's
    /// settings folder (tests, and trials that must not touch it). The views remembered for the
    /// project are read here; an unusable settings file adds [`crate::views::UNREADABLE`] to what
    /// opening reports and never fails the open.
    ///
    /// # Errors
    /// As [`Project::open_file`].
    pub fn open_file_in(
        s: &mut Session,
        path: &str,
        seed: u64,
        settings: crate::views::Settings,
    ) -> Result<(ProjectHandle, String), String> {
        let path = crate::file::normalize(path)?;
        let o = crate::file::Binding::open(&path, seed, &writer())?;
        let ledger = o.ledger;
        let default_layer =
            ledger.document().layers.iter().find(|(_, l)| l.name == "0").map(|(id, _)| *id).ok_or("no layer 0")?;
        let pages: Vec<PageId> = ledger.document().page_order.clone();
        // A fresh ID stream for this session, never reissuing the file's IDs.
        let ids =
            IdAllocator::seeded_after(seed, ledger.log().last().map_or(0, |t| t.id.0) ^ u128::from(ledger.head()));
        let baseline = ledger.head();
        let mut summary = vec![format!("Opened {path} at transaction {}.", ledger.head())];
        if o.report.outcome != ecad_file::Outcome::Clean {
            summary.push(o.report.describe());
        }
        if let Some(r) = &o.reclaimed_lock {
            summary.push(format!(
                "A lock left by a session that ended without closing the project ({r}) was reclaimed; nothing else is editing the file."
            ));
        }
        if let Some(r) = o.binding.status().read_only {
            summary.push(format!("Opened read-only: {r}. Save As can save a copy."));
        }
        // Cable views remembered by this user (brief 005); IDs not at the head are ignored.
        let loaded = settings.load(o.project);
        if loaded.unreadable {
            summary.push(crate::views::UNREADABLE.into());
        }
        let views: BTreeSet<CableId> =
            loaded.fan.into_iter().filter(|k| ledger.document().cables.contains_key(k)).collect();
        let summary = summary.join("\n");
        let project = Arc::new(Mutex::new(Project {
            name: file_stem(&path),
            path: Some(path.clone()),
            ledger,
            ids,
            id: o.project,
            store: MemoryStore::default(),
            file: o.binding,
            open_report: Some(o.report),
            pages: pages.iter().map(|p| (*p, PageView::default())).collect(),
            events: EventLog::default(),
            clips: BTreeMap::new(),
            faults: Faults::default(),
            persistence: Persistence::File,
            baseline,
            tolerance: 1e-6,
            default_layer,
            me: Weak::new(),
            unsaved_marker: Arc::new(Drawing::new_metric()),
            views: BTreeSet::new(),
            settings: crate::views::Settings::none(),
            said_not_remembered: false,
            author: Author::User,
        }));
        let mut p = crate::access::try_lock(&project)?;
        p.me = Arc::downgrade(&project);
        p.views = views;
        p.settings = settings;
        let head = p.ledger.head();
        p.events.record(EventKind::ProjectOpened, None, head, None, summary.clone());
        if let Some(first) = pages.first() {
            p.open_page(&mut Pages(s), *first)?;
        }
        drop(p);
        Ok((ProjectHandle::new(project), summary))
    }

    /// Where the project's file stands (`Persistence::File`).
    pub fn file_status(&self) -> crate::file::FileStatus {
        self.file.status()
    }

    /// Bind the project to the file at `path`: its name, and every open page's path and title.
    fn bind(&mut self, pg: &mut Pages<'_>, path: &str) {
        self.path = Some(path.to_string());
        self.name = file_stem(path);
        let pages: Vec<(PageId, u64)> = self.pages.iter().filter_map(|(p, v)| v.uid.map(|u| (*p, u))).collect();
        for (page, uid) in pages {
            let title = self.page_title(page);
            if let Some(st) = pg.doc_mut(uid) {
                st.path = self.path.clone();
                st.title = title;
            }
        }
    }

    /// Save or Save As to disk (`Persistence::File`).
    fn save_file(&mut self, pg: &mut Pages<'_>, path: Option<&str>, save_as: bool) -> cadcraft_engine::Result<Value> {
        let head = self.ledger.head();
        let what = if save_as { "saveas" } else { "qsave" };
        let target = path.filter(|p| !p.trim().is_empty());
        let r: Result<(Option<String>, Option<String>), String> = match target {
            None if save_as => Err(crate::file::SAVE_AS_NEEDS_PATH.into()),
            None => self.file.save(&mut self.ledger).map(|()| (None, None)),
            Some(p) => crate::file::normalize(p).and_then(|p| {
                let w = self.file.save_as(&p, self.id, &mut self.ledger, &writer(), now_ms())?;
                Ok((
                    Some(p),
                    match w {
                        crate::file::Written::Confirmed => None,
                        crate::file::Written::Unconfirmed(m) => Some(m),
                    },
                ))
            }),
        };
        let r = match r {
            Ok((new_path, unconfirmed)) => {
                if let Some(p) = &new_path {
                    self.bind(pg, p);
                }
                match unconfirmed {
                    None => Ok(()),
                    Some(m) => Err(m),
                }
            }
            Err(e) => Err(e),
        };
        let shown = self.path.clone().unwrap_or_default();
        let result = match r {
            Ok(()) => {
                let n = self.ledger.persisted();
                self.events.record(
                    EventKind::Saved,
                    Some(what),
                    head,
                    None,
                    format!("{shown} through transaction {n}"),
                );
                Ok(json!({"saved": true, "persisted": n, "path": shown}))
            }
            Err(e) => {
                self.events.record(EventKind::SaveFailed, Some(what), head, None, e.clone());
                Err(EngineError::Other(format!("{} was not saved: {e}", self.name)))
            }
        };
        self.sync_saved(pg);
        result
    }

    /// Project `page` with this project's cable views.
    fn project(&self, page: PageId, base: &Drawing, prev: &HandleMap) -> Result<(Drawing, HandleMap), String> {
        project_page_with(self.ledger.document(), page, base, prev, &self.views, self.tolerance)
    }

    /// The view `cable` is shown in.
    pub fn view_of(&self, cable: CableId) -> View {
        if self.views.contains(&cable) { View::Fan } else { View::Traditional }
    }

    /// Show `cable` in `view` (brief 005): presentation only, so no transaction, no head move, no
    /// unsaved change. Every open page is redrawn through the same path as after an undo, and the
    /// fan-view set is remembered in [`Project::settings`]. Returns whether the view changed and,
    /// when it changed, whether it was remembered (else why not).
    pub(crate) fn set_view(&mut self, pg: &mut Pages<'_>, cable: CableId, view: View) -> (bool, Result<(), String>) {
        let changed = match view {
            View::Fan => self.views.insert(cable),
            View::Traditional => self.views.remove(&cable),
        };
        if !changed {
            return (false, Ok(()));
        }
        // IDs no longer at the head are dropped from what is remembered.
        let doc = self.ledger.document();
        self.views.retain(|k| doc.cables.contains_key(k));
        self.refresh_pages(pg, Some("cableview"));
        (true, self.settings.store(self.id, &self.views))
    }

    fn owner_ref(&self) -> Option<OwnerRef> {
        let strong = self.me.upgrade()?;
        let r: OwnerRef = strong;
        Some(r)
    }

    /// Whether the project holds design work that is not saved: head versus persisted position
    /// (SAVE-01), or, with no persistence, head versus the position it was created at. A new file
    /// project that was never edited (only its creation transaction) and never written has
    /// nothing to lose; once a file exists, head versus persisted applies strictly.
    pub fn unsaved(&self) -> bool {
        self.unsaved_transactions() > 0
    }

    /// How many transactions the unsaved state covers (0 when nothing is unsaved).
    pub fn unsaved_transactions(&self) -> u64 {
        let from = match self.persistence {
            Persistence::TestStore => self.ledger.persisted(),
            Persistence::File if self.file.bound() => self.ledger.persisted(),
            Persistence::File => self.ledger.persisted().max(self.baseline),
            Persistence::Unavailable => self.baseline,
        };
        self.ledger.head().saturating_sub(from)
    }

    /// The page shown by engine document `uid`.
    pub fn page_of(&self, uid: u64) -> Option<PageId> {
        self.pages.iter().find(|(_, v)| v.uid == Some(uid)).map(|(p, _)| *p)
    }

    /// How many pages are open.
    pub fn open_pages(&self) -> usize {
        self.pages.values().filter(|v| v.uid.is_some()).count()
    }

    fn page_title(&self, page: PageId) -> String {
        let t = self.ledger.document().pages.get(&page).map(|p| p.title.clone()).unwrap_or_default();
        format!("{} - page {t}", self.name)
    }

    /// Open `page` as a CADCraft document (or switch to it), projecting it from the document.
    ///
    /// # Errors
    /// Unknown page or projection failure.
    pub(crate) fn open_page(&mut self, pg: &mut Pages<'_>, page: PageId) -> Result<u64, String> {
        if !self.ledger.document().pages.contains_key(&page) {
            return Err(format!("page {page} does not exist"));
        }
        if let Some(uid) = self.pages.get(&page).and_then(|v| v.uid)
            && pg.activate(uid)
        {
            self.refresh_if_stale(pg, page);
            return Ok(uid);
        }
        let (d, map) = self.project(page, &base_drawing(), &HandleMap::default())?;
        let title = self.page_title(page);
        let owner = self.owner_ref().ok_or("project is not shared")?;
        let uid = pg.open(d, &title, self.path.clone(), owner).ok_or("no document")?;
        let last = pg.doc(uid).map(|st| st.doc.clone());
        let view = self.pages.entry(page).or_default();
        *view = PageView { uid: Some(uid), map, last, unavailable: false, stale: false };
        self.events.record(EventKind::PageOpened, None, self.ledger.head(), Some(page), title);
        self.sync_saved(pg);
        Ok(uid)
    }

    /// Keep each open page's `saved` pointer in step with the project's unsaved state, so the
    /// engine's own pointer comparison agrees with SAVE-01.
    pub(crate) fn sync_saved(&self, pg: &mut Pages<'_>) {
        let unsaved = self.unsaved();
        for v in self.pages.values() {
            if let Some(st) = v.uid.and_then(|u| pg.doc_mut(u)) {
                st.saved = if unsaved { self.unsaved_marker.clone() } else { st.doc.clone() };
            }
        }
    }

    fn take_projection_fault(&mut self) -> bool {
        if self.faults.fail_projections > 0 {
            self.faults.fail_projections -= 1;
            true
        } else {
            false
        }
    }

    /// Bring every open page's drawing and map in line with the document (handles kept where
    /// objects remain). Pages that fail are rebuilt from scratch; if that fails too, the page view
    /// becomes unavailable (PROJ-02).
    pub(crate) fn refresh_pages(&mut self, pg: &mut Pages<'_>, command: Option<&str>) {
        // An unavailable page stays a placeholder until it is closed and reopened.
        let pages: Vec<PageId> =
            self.pages.iter().filter(|(_, v)| v.uid.is_some() && !v.unavailable).map(|(p, _)| *p).collect();
        for page in pages {
            self.refresh_page(pg, page, command, None, false);
        }
        self.sync_saved(pg);
    }

    /// Project one page; `committed` is the engine's drawing (with its new handles already in the
    /// map) after a CADCraft commit, used as the base and for the normal-path fidelity check.
    /// `recovery` marks a reprojection with fresh handles (owner recovery); it and any rebuild are
    /// checked by [`verify_page`] instead, which does not rely on handle numbers.
    pub(crate) fn refresh_page(
        &mut self,
        pg: &mut Pages<'_>,
        page: PageId,
        command: Option<&str>,
        committed: Option<(&Arc<Drawing>, &BTreeSet<Handle>)>,
        recovery: bool,
    ) {
        let Some(view) = self.pages.get(&page) else { return };
        let Some(uid) = view.uid else { return };
        let Some(current) = pg.doc(uid).map(|st| st.doc.clone()) else { return };
        let current = Some(current);
        let base = committed.map(|(d, _)| d.clone()).or(current).unwrap_or_else(|| Arc::new(base_drawing()));
        let prev = view.map.clone();
        let head = self.ledger.head();
        let first = if self.take_projection_fault() {
            Err("injected projection failure".to_string())
        } else {
            self.project(page, &base, &prev)
        };
        // Only a normal projection keeps the engine's handles, so only then is fidelity comparable.
        let normal = first.is_ok();
        let result = match first {
            Ok(r) => Ok(r),
            Err(e) => {
                self.events.record(EventKind::ProjectionFailed, command, head, Some(page), e);
                let rebuilt = if self.take_projection_fault() {
                    Err("injected rebuild failure".to_string())
                } else {
                    // Rebuild from scratch: fresh handles, nothing trusted from the failed state.
                    self.project(page, &base_drawing(), &HandleMap::default())
                };
                match &rebuilt {
                    Ok(_) => self.events.record(
                        EventKind::ProjectionRebuilt,
                        command,
                        head,
                        Some(page),
                        "drawing and map rebuilt from the document",
                    ),
                    Err(e) => self.events.record(EventKind::PageUnavailable, command, head, Some(page), e.clone()),
                }
                rebuilt
            }
        };
        // Recovery path: fresh handles, so the check is by content against the document.
        let result = match result {
            Ok((d, map)) if recovery || !normal => {
                let v = verify_page_with(self.ledger.document(), page, &d, &map, &self.views, self.tolerance);
                if v.ok() {
                    self.events.record(EventKind::RecoveryVerified, command, head, Some(page), v.summary());
                    Ok((d, map))
                } else {
                    self.events.record(EventKind::RecoveryVerificationFailed, command, head, Some(page), v.summary());
                    self.events.record(
                        EventKind::PageUnavailable,
                        command,
                        head,
                        Some(page),
                        "rebuilt page did not verify",
                    );
                    Err(v.summary())
                }
            }
            r => r,
        };
        let title = self.page_title(page);
        let Some(st) = pg.doc_mut(uid) else { return };
        match result {
            Ok((d, map)) => {
                if let Some((committed, amended)) = committed.filter(|_| normal) {
                    // Fan lines are recomputed from the document (a terminal moved by M2 moves
                    // them), so they are not compared with what the engine committed.
                    let mut amended = amended.clone();
                    amended.extend(prev.fan_handles().chain(map.fan_handles()));
                    // Reference texts are derived too (brief 010): moving a reference point or a
                    // member route moves or rewrites them.
                    amended.extend(prev.ref_text_handles().chain(map.ref_text_handles()));
                    let mismatched = fidelity_mismatches(committed, &d, &amended);
                    if !mismatched.is_empty() {
                        self.events.record(
                            EventKind::FidelityMismatch,
                            command,
                            head,
                            Some(page),
                            format!("{} objects differ from the projection; the projection was kept", mismatched.len()),
                        );
                    }
                }
                let changed = st.doc.model != d.model
                    || st.doc.groups != d.groups
                    || st.doc.blocks != d.blocks
                    || st.doc.layers != d.layers;
                if changed {
                    let keep: BTreeSet<Handle> = map.handles().collect();
                    st.selection.retain(|h| keep.contains(h));
                    st.doc = Arc::new(d);
                    st.revision += 1;
                }
                st.title = title;
                if let Some(v) = self.pages.get_mut(&page) {
                    v.map = map;
                    v.last = Some(st.doc.clone());
                    v.unavailable = false;
                    v.stale = false;
                }
            }
            Err(_) => {
                st.doc = Arc::new(placeholder_drawing());
                st.title = format!("{title}{UNAVAILABLE_SUFFIX}");
                st.selection.clear();
                st.revision += 1;
                if let Some(v) = self.pages.get_mut(&page) {
                    v.map = HandleMap::default();
                    v.last = Some(st.doc.clone());
                    v.unavailable = true;
                    v.stale = false;
                }
                pg.echo("This page could not be drawn; it is unavailable until it is closed and reopened. The project is intact and can be saved.");
            }
        }
    }

    /// Redraw `page` if a commit on another page left it stale ([`PageView::stale`]).
    pub(crate) fn refresh_if_stale(&mut self, pg: &mut Pages<'_>, page: PageId) {
        if self.pages.get(&page).is_some_and(|v| v.stale && v.uid.is_some() && !v.unavailable) {
            self.refresh_page(pg, page, None, None, false);
            self.sync_saved(pg);
        }
    }

    /// Commit ECAD-made ops (Path A) and refresh the open pages.
    ///
    /// # Errors
    /// Validation failure; nothing changes.
    pub(crate) fn apply_edit(
        &mut self,
        pg: &mut Pages<'_>,
        command: &str,
        label: &str,
        ops: Vec<Op>,
    ) -> Result<ecad_model::TxId, String> {
        if pg.command_running() {
            return Err("finish or cancel the current command first (TX-05)".into());
        }
        #[cfg(feature = "fault-injection")]
        if std::mem::take(&mut self.faults.panic_in_command) {
            crate::fault_panic();
        }
        // References are derived for the cables the ops could touch only (brief 014).
        let scope = crate::references::cables_touched(self.ledger.document(), &ops);
        let refs_before = crate::references::Snapshot::take(self.ledger.document(), scope.as_ref());
        let tx = self.ledger.commit_at(self.author.clone(), label, ops, now_ms()).map_err(|e| {
            self.events.record(EventKind::Refused, Some(command), self.ledger.head(), None, e.to_string());
            e.to_string()
        })?;
        self.events.record(EventKind::Accepted, Some(command), self.ledger.head(), None, label);
        self.refresh_pages(pg, Some(command));
        let refs_after = crate::references::Snapshot::take(self.ledger.document(), scope.as_ref());
        if let Some(line) = refs_before.orphaned_line(self.ledger.document(), &refs_after) {
            pg.echo(line);
        }
        Ok(tx)
    }

    /// Echo the line of design 010 §4.3 when references linked before a change (`before`) have
    /// lost their other end.
    pub(crate) fn say_orphaned(&self, pg: &mut Pages<'_>, before: &BTreeSet<RepresentationId>) {
        if before.is_empty() {
            return;
        }
        if let Some(line) = crate::references::orphaned_line(self.ledger.document(), before) {
            pg.echo(line);
        }
    }

    /// Restore the last projection of `page` after a refusal.
    fn restore(&self, pg: &mut Pages<'_>, uid: u64, page: PageId) {
        if let (Some(st), Some(last)) = (pg.doc_mut(uid), self.pages.get(&page).and_then(|v| v.last.clone())) {
            st.doc = last;
            st.selection.clear();
            st.revision += 1;
        }
    }

    /// Facts for an inspection snapshot: per open page, whether the drawing equals a fresh
    /// projection with the same handles.
    pub fn live_facts(&self, pg: &PagesRef<'_>) -> LiveFacts {
        let doc = self.ledger.document();
        let pages = doc
            .page_order
            .iter()
            .map(|p| {
                let view = self.pages.get(p);
                let st = view.and_then(|v| v.uid).and_then(|u| pg.doc(u));
                let matches = match (view, st) {
                    (Some(v), Some(st)) => Some(
                        project_page_with(doc, *p, &st.doc, &v.map, &self.views, self.tolerance)
                            .map(|(d, m)| d.model == st.doc.model && m == v.map && v.map.is_consistent())
                            .unwrap_or(false),
                    ),
                    _ => None,
                };
                PageInfo {
                    id: *p,
                    title: doc.pages.get(p).map(|x| x.title.clone()).unwrap_or_default(),
                    open: st.is_some(),
                    projection_matches: matches,
                    mapped_handles: view.filter(|_| st.is_some()).map(|v| v.map.len()),
                }
            })
            .collect();
        LiveFacts {
            pages,
            upstream: Some(crate::UPSTREAM_COMMIT.into()),
            persistence: Some(self.file_facts().to_json()),
        }
    }

    /// A JSON snapshot (live when `at` is `None`).
    ///
    /// # Errors
    /// See [`ecad_inspect::snapshot`].
    pub fn snapshot(&self, pg: &PagesRef<'_>, at: Option<u64>) -> Result<Value, String> {
        let facts = self.live_facts(pg);
        ecad_inspect::snapshot(&self.name, &self.ledger, at, &self.events, at.is_none().then_some(&facts))
            .map_err(|e| e.to_string())
    }

    /// Positions of attached route ends that follow the dragged terminals (drag preview, M2).
    fn following_routes(
        &self,
        page: PageId,
        moved: &dyn Fn(TerminalId) -> Option<Pt>,
    ) -> Vec<cadcraft_engine::doc::EntityKind> {
        let doc = self.ledger.document();
        let mut out = Vec::new();
        for (cid, c) in &doc.conductors {
            let Some((_, prim)) = ecad_model::derive::route_on_page(doc, *cid, page) else { continue };
            let mut p = prim.clone();
            let mut any = false;
            for end in End::BOTH {
                if let Some(to) = c.end(end).and_then(moved)
                    && let Some(q) = p.with_path_end(end, to)
                {
                    p = q;
                    any = true;
                }
            }
            if any {
                out.push(kind_for_primitive(&p));
            }
        }
        out
    }
}

/// Handles whose entity differs between the engine's committed drawing and the projection, other
/// than those the owner amended on purpose.
fn fidelity_mismatches(committed: &Drawing, projected: &Drawing, amended: &BTreeSet<Handle>) -> Vec<Handle> {
    let a: HashMap<Handle, &Arc<Entity>> = committed.model.iter().map(|e| (e.handle, e)).collect();
    let b: HashMap<Handle, &Arc<Entity>> = projected.model.iter().map(|e| (e.handle, e)).collect();
    let mut out: Vec<Handle> =
        a.iter().filter(|(h, e)| !amended.contains(h) && b.get(h).is_none_or(|x| x != *e)).map(|(h, _)| *h).collect();
    out.extend(b.keys().filter(|h| !a.contains_key(h) && !amended.contains(h)));
    out.sort();
    out
}

/// Commands refused on ECAD pages before they start (owner decision, 2026-10-31, brief 007): the
/// Block Editor and in-place reference editing swap or rewrite the page drawing and its blocks
/// outside the commit gate, and APPLOAD and LISP evaluate LISP (see [`LISP_REFUSED`]).
pub const REFUSED_COMMANDS: &[&str] =
    &["bedit", "bsave", "bsaveas", "bclose", "refedit", "refset", "refclose", "appload", "lisp"];

/// Why LISP is refused on ECAD pages: its built-ins change the drawing outside any command, and a
/// routine keeps an undo step of its own.
pub const LISP_REFUSED: &str =
    "LISP is not available on ECAD pages: it changes the drawing outside the commands ECAD records";

impl DocumentOwner for Project {
    fn refuse_start(&self, _s: &Session, uid: u64, start: Start<'_>) -> Option<String> {
        self.page_of(uid)?;
        match start {
            Start::Lisp => Some(LISP_REFUSED.into()),
            Start::Command("appload" | "lisp") => Some(LISP_REFUSED.into()),
            Start::Command(id) if REFUSED_COMMANDS.contains(&id) => Some(format!(
                "{} is not available on ECAD pages: it edits blocks outside the commands ECAD records",
                id.to_ascii_uppercase()
            )),
            Start::Command(_) => None,
        }
    }

    fn commit(&mut self, s: &mut Session, c: &Commit<'_>) -> Verdict {
        let pg = &mut Pages(s);
        #[cfg(feature = "fault-injection")]
        if std::mem::take(&mut self.faults.panic_in_commit) {
            crate::fault_panic();
        }
        let head = self.ledger.head();
        let Some(page) = self.page_of(c.uid) else { return Verdict::Accept };
        if let Some(clip) = c.clipboard
            && clip.uid == c.uid
        {
            self.clips.insert(
                clip.serial,
                ClipSnapshot {
                    doc: self.ledger.document().clone(),
                    page,
                    map: self.pages.get(&page).map(|v| v.map.clone()).unwrap_or_default(),
                },
            );
            while self.clips.len() > 8 {
                self.clips.pop_first();
            }
            self.events.record(
                EventKind::ClipboardCaptured,
                Some(c.command),
                head,
                Some(page),
                format!("{} objects", clip.handles.len()),
            );
        }
        let Some(st) = pg.doc(c.uid) else { return Verdict::Accept };
        let now = st.doc.clone();
        let Some(view) = self.pages.get(&page) else { return Verdict::Accept };
        let Some(last) = view.last.clone() else { return Verdict::Accept };
        if Arc::ptr_eq(&now, &last) {
            return Verdict::Accept;
        }
        if view.unavailable {
            self.restore(pg, c.uid, page);
            return Verdict::Refuse(crate::commands::UNAVAILABLE.into());
        }
        let t = translate::translate(&mut translate::Ctx {
            ledger: &mut self.ledger,
            page,
            map: &view.map,
            last: &last,
            now: &now,
            provenance: c.provenance,
            clips: &self.clips,
            ids: &mut self.ids,
            tol: self.tolerance,
        });
        let t = match t {
            Ok(t) => t,
            Err(m) => {
                self.restore(pg, c.uid, page);
                self.events.record(EventKind::Refused, Some(c.command), head, Some(page), m.clone());
                return Verdict::Refuse(m);
            }
        };
        if t.ops.is_empty() {
            // Only differences that are not recorded (header variables such as the current layer).
            if let Some(v) = self.pages.get_mut(&page) {
                v.last = Some(now);
            }
            self.events.record(EventKind::NoChange, Some(c.command), head, Some(page), "");
            return Verdict::Accept;
        }
        let label = c.command.to_ascii_uppercase();
        // References are derived for the cables the ops could touch only, once before and once
        // after the commit (brief 014): a move of a component outside any cable derives none.
        let scope = crate::references::cables_touched(self.ledger.document(), &t.ops);
        let refs_before = crate::references::Snapshot::take(self.ledger.document(), scope.as_ref());
        if let Err(e) = self.ledger.commit_at(self.author.clone(), &label, t.ops, now_ms()) {
            self.restore(pg, c.uid, page);
            let m = format!("not recorded: {e}");
            self.events.record(EventKind::Refused, Some(c.command), head, Some(page), m.clone());
            return Verdict::Refuse(m);
        }
        self.events.record(EventKind::Accepted, Some(c.command), self.ledger.head(), Some(page), t.summary);
        // Accepted: from here on consistency is restored by projecting the document (PROJ-02).
        if let Some(v) = self.pages.get_mut(&page) {
            for (id, h) in &t.new_items {
                v.map.insert_item(*id, *h);
            }
            for ((r, term), h) in &t.new_markers {
                v.map.insert_marker(*r, *term, *h);
            }
        }
        self.refresh_page(pg, page, Some(c.command), Some((&now, &t.amended)), false);
        // A reference end on another open page may be drawn from this page's objects (brief 010).
        // Only the page being shown is redrawn on a commit (owner, 2026-11-08): another open page
        // whose reference ends the commit changed (every other page when the ops could touch any
        // cable) is marked stale and redrawn when it comes to the front.
        let refs_after = crate::references::Snapshot::take(self.ledger.document(), scope.as_ref());
        let changed = refs_before.pages_changed(&refs_after);
        for (p, v) in self.pages.iter_mut() {
            if *p != page && v.uid.is_some() && !v.unavailable && changed.as_ref().is_none_or(|c| c.contains(p)) {
                v.stale = true;
            }
        }
        if let Some(line) = refs_before.orphaned_line(self.ledger.document(), &refs_after) {
            pg.echo(line);
        }
        for note in &t.notes {
            pg.echo(note.clone());
        }
        self.sync_saved(pg);
        Verdict::Accept
    }

    fn undo(&mut self, s: &mut Session, _uid: u64) -> cadcraft_engine::Result<Option<String>> {
        self.history(&mut Pages(s), true)
    }

    fn redo(&mut self, s: &mut Session, _uid: u64) -> cadcraft_engine::Result<Option<String>> {
        self.history(&mut Pages(s), false)
    }

    fn save(
        &mut self,
        s: &mut Session,
        _uid: u64,
        path: Option<&str>,
        save_as: bool,
    ) -> cadcraft_engine::Result<Value> {
        let pg = &mut Pages(s);
        let head = self.ledger.head();
        if self.persistence == Persistence::File {
            return self.save_file(pg, path, save_as);
        }
        if self.persistence == Persistence::Unavailable {
            // Nothing is recorded as saved: no path, no store, no clean state.
            let what = if save_as { "saveas" } else { "qsave" };
            self.events.record(EventKind::SaveUnavailable, Some(what), head, None, SAVE_UNAVAILABLE);
            self.sync_saved(pg);
            return Err(EngineError::Other(SAVE_UNAVAILABLE.into()));
        }
        // The test store is in memory; the path is only recorded (no file is written), and only
        // once the save succeeded, so a failed Save As keeps the previous binding (§20.5).
        match self.store.save(self.id, &mut self.ledger, &writer()) {
            Ok(n) => {
                if let Some(p) = path {
                    self.path = Some(p.to_string());
                    for v in self.pages.values() {
                        if let Some(st) = v.uid.and_then(|u| pg.doc_mut(u)) {
                            st.path = self.path.clone();
                        }
                    }
                }
                self.events.record(EventKind::Saved, Some("qsave"), head, None, format!("persisted through seq {n}"));
                self.sync_saved(pg);
                Ok(json!({"saved": true, "persisted": n}))
            }
            Err(e) => {
                self.events.record(EventKind::SaveFailed, Some("qsave"), head, None, e.to_string());
                self.sync_saved(pg);
                Err(EngineError::Other(format!("{} was not saved: {e}", self.name)))
            }
        }
    }

    fn is_unsaved(&self) -> bool {
        self.unsaved()
    }

    fn close_needs_prompt(&self, _uid: u64) -> bool {
        // Closing a page tab never prompts; closing the last open page closes the project
        // (experiment choice, proposal §18.4), which prompts when unsaved.
        self.unsaved() && self.open_pages() <= 1
    }

    fn closed(&mut self, _s: &mut Session, uid: u64) -> bool {
        if let Some(page) = self.page_of(uid) {
            if let Some(v) = self.pages.get_mut(&page) {
                *v = PageView::default();
            }
            self.events.record(EventKind::PageClosed, Some("close"), self.ledger.head(), Some(page), "");
        }
        let done = self.open_pages() == 0;
        if done {
            // The project is closed: let go of its file and writer lock.
            self.file.release();
        }
        done
    }

    fn drag_extra(&self, s: &Session, uid: u64, drag: &cadcraft_engine::Drag) -> Vec<cadcraft_engine::doc::EntityKind> {
        let Some(page) = self.page_of(uid) else { return Vec::new() };
        let Some(view) = self.pages.get(&page) else { return Vec::new() };
        if drag.clipboard {
            return Vec::new();
        }
        let doc = self.ledger.document();
        let dragged: BTreeSet<Handle> = drag.handles.iter().copied().collect();
        let mut moved: HashMap<TerminalId, Pt> = HashMap::new();
        for (t, rep, at) in ecad_model::derive::terminal_positions(doc, page) {
            let carried = match doc.representations.get(&rep).map(|r| r.form) {
                Some(Form::Drawn) => view.map.marker(rep, t).is_some_and(|h| dragged.contains(&h)),
                Some(Form::Symbol) => doc
                    .representations
                    .get(&rep)
                    .and_then(|r| r.items.first())
                    .and_then(|i| view.map.item(*i))
                    .is_some_and(|h| dragged.contains(&h)),
                None => false,
            };
            if carried {
                let p = drag.matrix.apply(cadcraft_engine::geom::Vec2::new(at.x, at.y));
                moved.insert(t, Pt::new(p.x, p.y));
            }
        }
        let _ = s;
        if moved.is_empty() {
            return Vec::new();
        }
        self.following_routes(page, &|t| moved.get(&t).copied())
    }

    /// Terminal rings and wire-end marks over an open page (brief 008, design §4.1).
    /// Only while the Model tab is shown: the marks are at model positions, and a paper-space
    /// layout (also while working in one of its viewports) shows the page at another place and
    /// scale, or not at all.
    fn overlay(&self, s: &Session, uid: u64) -> Vec<cadcraft_engine::ext::Mark> {
        if s.layout_space() != cadcraft_engine::doc::Space::Model {
            return Vec::new();
        }
        crate::interact::overlay_marks(self, uid)
    }

    fn recover(&mut self, s: &mut Session) {
        let pg = &mut Pages(s);
        // Rebuild every open page from the authoritative document with fresh maps.
        for v in self.pages.values_mut() {
            v.map = HandleMap::default();
            v.last = None;
        }
        let pages: Vec<PageId> = self.pages.iter().filter(|(_, v)| v.uid.is_some()).map(|(p, _)| *p).collect();
        for page in pages {
            if let Some(uid) = self.pages.get(&page).and_then(|v| v.uid)
                && let Some(st) = pg.doc_mut(uid)
            {
                st.doc = Arc::new(base_drawing());
            }
            self.refresh_page(pg, page, None, None, true);
        }
        self.sync_saved(pg);
        self.events.record(
            EventKind::Recovered,
            None,
            self.ledger.head(),
            None,
            "owner callback failed; pages rebuilt from the document",
        );
    }

    fn label(&self) -> String {
        self.name.clone()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Project {
    fn history(&mut self, pg: &mut Pages<'_>, undo: bool) -> cadcraft_engine::Result<Option<String>> {
        let command = if undo { "undo" } else { "redo" };
        if self.pages.values().any(|v| v.unavailable) {
            return Err(EngineError::Other("close and reopen the unavailable page first".into()));
        }
        let linked = crate::references::linked(self.ledger.document());
        let r = if undo {
            self.ledger.undo_at(self.author.clone(), now_ms())
        } else {
            self.ledger.redo_at(self.author.clone(), now_ms())
        };
        match r {
            Ok(_) => {
                let label = self.ledger.log().last().map(|t| t.label.clone()).unwrap_or_default();
                let kind = if undo { EventKind::Undo } else { EventKind::Redo };
                self.events.record(kind, Some(command), self.ledger.head(), None, label.clone());
                self.refresh_pages(pg, Some(command));
                self.say_orphaned(pg, &linked);
                Ok(Some(label))
            }
            Err(ecad_model::ModelError::NothingTo(_)) => Ok(None),
            Err(e) => Err(EngineError::Other(e.to_string())),
        }
    }
}
