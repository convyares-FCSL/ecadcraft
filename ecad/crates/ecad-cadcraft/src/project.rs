//! The ECAD project as a CADCraft document owner.
//!
//! [`Project`] holds the authoritative [`Ledger`] and one view per page. A page that is open in the
//! engine is a CADCraft document whose drawing is a projection of the ECAD document; the
//! [`HandleMap`] relates its handles to ECAD IDs and lives only in the session (proposal §13).

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, Weak};

use cadcraft_engine::doc::{
    Block, Common, Drawing, Entity, EntityKind, EntityStore, Group, Handle, Layer as CLayer, Text,
};
use cadcraft_engine::ext::{Commit, DocumentOwner, OwnerRef, Verdict};
use cadcraft_engine::{EngineError, Session};

use crate::access::{Pages, PagesRef, ProjectHandle};
use ecad_inspect::{EventKind, EventLog, LiveFacts, PageInfo};
use ecad_model::{
    Author, Document, End, Form, IdAllocator, ItemKind, Layer, LayerId, Ledger, Op, Page, PageId, Pt, RepresentationId,
    Subject, TerminalId,
};
use ecad_store::{MemoryMedium, TestStore};
use serde_json::{Value, json};

use crate::convert::{ECAD_PREFIX, MARKER_LAYER, block_name, entity_for, kind_for_primitive, marker_entity};
use crate::translate;
use crate::verify::verify_page;

/// What a projected handle stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mapped {
    Item(ecad_model::ItemId),
    /// A terminal marker of a drawn representation.
    Marker(RepresentationId, TerminalId),
}

/// ECAD ID ⇄ CADCraft handle for one page. Session-only, never saved.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HandleMap {
    by_handle: HashMap<Handle, Mapped>,
    items: BTreeMap<ecad_model::ItemId, Handle>,
    markers: BTreeMap<(RepresentationId, TerminalId), Handle>,
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
        self.by_handle.len() == self.items.len() + self.markers.len()
            && self.items.iter().all(|(id, h)| self.by_handle.get(h) == Some(&Mapped::Item(*id)))
            && self.markers.iter().all(|((r, t), h)| self.by_handle.get(h) == Some(&Mapped::Marker(*r, *t)))
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
}

/// Injected failures for the experiment (never set in normal use).
#[derive(Debug, Clone, Default)]
pub struct Faults {
    /// The next `n` projection attempts fail.
    pub fail_projections: u32,
    /// The next commit callback panics (`fault-injection` feature only).
    #[cfg(feature = "fault-injection")]
    pub panic_in_commit: bool,
    /// The next ECAD command panics before committing (`fault-injection` feature only).
    #[cfg(feature = "fault-injection")]
    pub panic_in_command: bool,
}

/// Whether a project can be saved, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persistence {
    /// Automated tests only: saving appends to the in-memory test store and marks the project
    /// clean. No file is written.
    TestStore,
    /// No persistence (the desktop trial): Save and Save As report that saving is unavailable,
    /// change nothing and never mark the project clean.
    Unavailable,
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
    pub store: TestStore<MemoryMedium>,
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
    let mut map = HandleMap::default();
    let mut ents: Vec<Entity> = Vec::new();
    for (id, item) in doc.items.iter().filter(|(_, i)| i.page == page) {
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
                ],
                now_ms(),
            )
            .map_err(|e| e.to_string())?;
        let baseline = ledger.head();
        let project = Arc::new(Mutex::new(Project {
            name: name.to_string(),
            path: None,
            ledger,
            ids,
            store: TestStore::new(MemoryMedium::default()),
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
        }));
        let mut p = crate::access::try_lock(&project)?;
        p.me = Arc::downgrade(&project);
        p.pages.insert(page, PageView::default());
        p.open_page(&mut Pages(s), page)?;
        drop(p);
        Ok(ProjectHandle::new(project))
    }

    /// Reopen a project from its store (the log is replayed; FILE-02).
    ///
    /// # Errors
    /// Store or replay failure.
    pub(crate) fn open_from(
        s: &mut Session,
        name: &str,
        store: TestStore<MemoryMedium>,
        seed: u64,
        use_checkpoint: bool,
    ) -> Result<ProjectHandle, String> {
        let ledger = if use_checkpoint { store.open_with_checkpoint(seed) } else { store.open(seed) }
            .map_err(|e| e.to_string())?;
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
            store,
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
        }));
        let mut p = crate::access::try_lock(&project)?;
        p.me = Arc::downgrade(&project);
        if let Some(first) = pages.first() {
            p.open_page(&mut Pages(s), *first)?;
        }
        drop(p);
        Ok(ProjectHandle::new(project))
    }

    fn owner_ref(&self) -> Option<OwnerRef> {
        let strong = self.me.upgrade()?;
        let r: OwnerRef = strong;
        Some(r)
    }

    /// Whether the project holds design work that is not saved: head versus persisted position
    /// (SAVE-01), or, with no persistence, head versus the position it was created at.
    pub fn unsaved(&self) -> bool {
        match self.persistence {
            Persistence::TestStore => self.ledger.is_unsaved(),
            Persistence::Unavailable => self.ledger.head() != self.baseline,
        }
    }

    /// How many transactions the unsaved state covers (0 when nothing is unsaved).
    pub fn unsaved_transactions(&self) -> u64 {
        let from = match self.persistence {
            Persistence::TestStore => self.ledger.persisted(),
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
            return Ok(uid);
        }
        let (d, map) = project_page(self.ledger.document(), page, &base_drawing(), &HandleMap::default())?;
        let title = self.page_title(page);
        let owner = self.owner_ref().ok_or("project is not shared")?;
        let uid = pg.open(d, &title, self.path.clone(), owner).ok_or("no document")?;
        let last = pg.doc(uid).map(|st| st.doc.clone());
        let view = self.pages.entry(page).or_default();
        *view = PageView { uid: Some(uid), map, last, unavailable: false };
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
            project_page(self.ledger.document(), page, &base, &prev)
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
                    project_page(self.ledger.document(), page, &base_drawing(), &HandleMap::default())
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
                let v = verify_page(self.ledger.document(), page, &d, &map);
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
                    let mismatched = fidelity_mismatches(committed, &d, amended);
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
                }
                pg.echo("This page could not be drawn; it is unavailable until it is closed and reopened. The project is intact and can be saved.");
            }
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
        let tx = self.ledger.commit_at(Author::User, label, ops, now_ms()).map_err(|e| {
            self.events.record(EventKind::Refused, Some(command), self.ledger.head(), None, e.to_string());
            e.to_string()
        })?;
        self.events.record(EventKind::Accepted, Some(command), self.ledger.head(), None, label);
        self.refresh_pages(pg, Some(command));
        Ok(tx)
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
                        project_page(doc, *p, &st.doc, &v.map)
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
        LiveFacts { pages, upstream: Some(crate::UPSTREAM_COMMIT.into()) }
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

impl DocumentOwner for Project {
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
            doc: self.ledger.document(),
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
        if let Err(e) = self.ledger.commit_at(Author::User, &label, t.ops, now_ms()) {
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
        if self.persistence == Persistence::Unavailable {
            // Nothing is recorded as saved: no path, no store, no clean state.
            let what = if save_as { "saveas" } else { "qsave" };
            self.events.record(EventKind::SaveUnavailable, Some(what), head, None, SAVE_UNAVAILABLE);
            self.sync_saved(pg);
            return Err(EngineError::Other(SAVE_UNAVAILABLE.into()));
        }
        // The experiment's store is in memory; the path is only recorded (no file is written).
        if let Some(p) = path {
            self.path = Some(p.to_string());
            for v in self.pages.values() {
                if let Some(st) = v.uid.and_then(|u| pg.doc_mut(u)) {
                    st.path = self.path.clone();
                }
            }
        }
        match self.store.save(&mut self.ledger) {
            Ok(n) => {
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
        self.open_pages() == 0
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
        let r = if undo {
            self.ledger.undo_at(Author::User, now_ms())
        } else {
            self.ledger.redo_at(Author::User, now_ms())
        };
        match r {
            Ok(_) => {
                let label = self.ledger.log().last().map(|t| t.label.clone()).unwrap_or_default();
                let kind = if undo { EventKind::Undo } else { EventKind::Redo };
                self.events.record(kind, Some(command), self.ledger.head(), None, label.clone());
                self.refresh_pages(pg, Some(command));
                Ok(Some(label))
            }
            Err(ecad_model::ModelError::NothingTo(_)) => Ok(None),
            Err(e) => Err(EngineError::Other(e.to_string())),
        }
    }
}
