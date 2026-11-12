//! The ECAD project as a CADCraft document owner.
//!
//! [`Project`] holds the authoritative [`Ledger`] and one view per page. A page that is open in the
//! engine is a CADCraft document whose drawing is a projection of the ECAD document; the
//! [`HandleMap`] relates its handles to ECAD IDs and lives only in the session (proposal §13).

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, Weak};

use cadcraft_engine::doc::{Block, Common, Drawing, Entity, EntityKind, EntityStore, Group, Handle, Layer as CLayer, Lineweight, Text};
use cadcraft_engine::ext::{Commit, DocumentOwner, OwnerRef, Start, Verdict};
use cadcraft_engine::{EngineError, Session};

use crate::access::{Pages, PagesRef, ProjectHandle};
use ecad_file::{OpenReport, ProjectFile, SaveOutcome};
use ecad_inspect::{EventKind, EventLog, LiveFacts, PageInfo};
use ecad_model::{
    Author, CableId, ComponentId, ConductorId, Document, End, Form, IdAllocator, ItemKind, Layer, LayerId, Ledger, LengthUnit, Op, Page, PageId,
    ProjectId, ProjectSettings, Pt, RepresentationId, Subject, TerminalId,
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
    /// A device's derived tag text or terminal label beside one of its symbols (brief 018), by
    /// the symbol representation: computed, not an item. Erasing it erases its representation;
    /// other edits of it alone are refused.
    Tag(RepresentationId),
    /// A conductor's derived label (brief 022): computed, not an item. Left out of an edit that
    /// also touches other objects; an edit of it alone is refused.
    Label(ConductorId),
    /// A line or text of the page's sheet frame or title block (brief 019), by its index among
    /// the frame's parts: computed, not an item. Every edit of it is refused.
    Frame(u32),
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
    /// Tag texts and terminal labels (brief 018): (symbol representation, which text) → handle.
    tags: BTreeMap<(RepresentationId, crate::devices::TextOf), Handle>,
    /// Conductor labels (brief 022): route representation → (conductor, handle).
    labels: BTreeMap<RepresentationId, (ConductorId, Handle)>,
    /// The sheet frame's parts (brief 019): index → handle.
    frame: BTreeMap<u32, Handle>,
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
    pub fn insert_part(&mut self, cable: CableId, index: u32, h: Handle, fan: bool, core: Option<ConductorId>, reference: Option<RepresentationId>) {
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
    /// The handle of a derived tag text or terminal label.
    pub fn tag_text(&self, rep: RepresentationId, of: crate::devices::TextOf) -> Option<Handle> {
        self.tags.get(&(rep, of)).copied()
    }
    pub fn insert_tag_text(&mut self, rep: RepresentationId, of: crate::devices::TextOf, h: Handle) {
        self.tags.insert((rep, of), h);
        self.by_handle.insert(h, Mapped::Tag(rep));
    }
    /// Every handle of a tag text or terminal label.
    pub fn tag_text_handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.tags.values().copied()
    }
    /// The handle of a conductor label, by its route representation.
    pub fn label(&self, rep: RepresentationId) -> Option<Handle> {
        self.labels.get(&rep).map(|(_, h)| *h)
    }
    pub fn insert_label(&mut self, rep: RepresentationId, conductor: ConductorId, h: Handle) {
        self.labels.insert(rep, (conductor, h));
        self.by_handle.insert(h, Mapped::Label(conductor));
    }
    /// Every handle of a conductor label.
    pub fn label_handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.labels.values().map(|(_, h)| *h)
    }
    /// The handle of frame part `index`.
    pub fn frame_part(&self, index: u32) -> Option<Handle> {
        self.frame.get(&index).copied()
    }
    pub fn insert_frame(&mut self, index: u32, h: Handle) {
        self.frame.insert(index, h);
        self.by_handle.insert(h, Mapped::Frame(index));
    }
    /// Every handle of the sheet frame.
    pub fn frame_handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.frame.values().copied()
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
        self.by_handle.len()
            == self.items.len() + self.markers.len() + self.fans.len() + self.refs.len() + self.tags.len() + self.labels.len() + self.frame.len()
            && self.items.iter().all(|(id, h)| self.by_handle.get(h) == Some(&Mapped::Item(*id)))
            && self.markers.iter().all(|((r, t), h)| self.by_handle.get(h) == Some(&Mapped::Marker(*r, *t)))
            && self.fans.iter().all(|((k, _), h)| self.by_handle.get(h) == Some(&Mapped::Fan(*k)))
            && self.refs.iter().all(|(r, h)| self.by_handle.get(h) == Some(&Mapped::RefText(*r)))
            && self.tags.iter().all(|((r, _), h)| self.by_handle.get(h) == Some(&Mapped::Tag(*r)))
            && self.labels.values().all(|(c, h)| self.by_handle.get(h) == Some(&Mapped::Label(*c)))
            && self.frame.iter().all(|(i, h)| self.by_handle.get(h) == Some(&Mapped::Frame(*i)))
    }
}

/// A page and, when it is the active page, the engine document showing it (the project's one
/// page window, brief 016: exactly one page is shown and editable at a time).
#[derive(Debug, Clone, Default)]
pub struct PageView {
    pub uid: Option<u64>,
    pub map: HandleMap,
    /// The drawing the owner last wrote (or accepted) for this page.
    pub last: Option<Arc<Drawing>>,
    /// Projection and rebuild both failed: the page must be closed and reopened (PROJ-02).
    pub unavailable: bool,
}

/// One page as the Project panel lists it ([`Project::page_rows`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRow {
    pub id: PageId,
    /// The number the page is shown with ([`crate::references::page_number`]).
    pub number: String,
    pub title: String,
    /// Its projection failed (PROJ-02).
    pub unavailable: bool,
    /// Its folder depth (0 at the top level) and folder (brief 019).
    pub depth: usize,
    pub folder: Option<ecad_model::FolderId>,
    /// Another page is shown with the same number (only from old data).
    pub duplicate: bool,
}

/// One row of the Pages tree (design 019 §5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeRow {
    Folder {
        id: ecad_model::FolderId,
        name: String,
        /// 1 at the top level.
        depth: usize,
        /// Pages inside, all levels.
        pages: usize,
        parent: Option<ecad_model::FolderId>,
    },
    Page(PageRow),
}

/// The Pages tree of `doc` (see [`Project::tree_rows`]); `unavailable` says which pages failed.
pub fn tree_rows(doc: &Document, unavailable: impl Fn(PageId) -> bool) -> Vec<TreeRow> {
    use ecad_model::book::{Book, Node};
    let book = Book::of(doc);
    let numbers: Vec<(PageId, String)> = book.order.iter().map(|p| (*p, book.number(doc, *p))).collect();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (_, n) in &numbers {
        *seen.entry(n.to_ascii_lowercase()).or_default() += 1;
    }
    // Pages per folder, all levels: count each page up its folder chain.
    let mut counts: HashMap<ecad_model::FolderId, usize> = HashMap::new();
    for p in &book.order {
        let mut at = ecad_model::book::folder_of(doc, *p).filter(|f| doc.folders.contains_key(f));
        let mut steps = 0;
        while let Some(f) = at {
            *counts.entry(f).or_default() += 1;
            at = doc.folders.get(&f).and_then(|x| x.parent);
            steps += 1;
            if steps > ecad_model::book::MAX_FOLDER_DEPTH {
                break;
            }
        }
    }
    let number: HashMap<PageId, &String> = numbers.iter().map(|(p, n)| (*p, n)).collect();
    book.nodes
        .iter()
        .map(|n| match n {
            Node::Folder { id, depth } => TreeRow::Folder {
                id: *id,
                name: doc.folders.get(id).map(|f| f.name.clone()).unwrap_or_default(),
                depth: *depth,
                pages: counts.get(id).copied().unwrap_or(0),
                parent: doc.folders.get(id).and_then(|f| f.parent),
            },
            Node::Page { id, depth, position } => {
                let num = number.get(id).map(|s| s.to_string()).unwrap_or_else(|| position.to_string());
                TreeRow::Page(PageRow {
                    id: *id,
                    duplicate: seen.get(&num.to_ascii_lowercase()).is_some_and(|c| *c > 1),
                    number: num,
                    title: doc.pages.get(id).map(|p| p.title.clone()).unwrap_or_default(),
                    unavailable: unavailable(*id),
                    depth: *depth,
                    folder: ecad_model::book::folder_of(doc, *id).filter(|f| doc.folders.contains_key(f)),
                })
            }
        })
        .collect()
}

/// How many recent changes [`Project::changes_since`] can answer for.
const CHANGE_LOG: usize = 64;

/// One change the project went through (a transaction, an undo or redo, a view switch): its
/// number and the pages whose drawing it could change (`None`: any page). Session only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub n: u64,
    pub pages: Option<BTreeSet<PageId>>,
    /// What it could change in the Devices list ([`crate::device_list::devices_touched`]).
    pub devices: crate::device_list::DeviceTouch,
    /// The objects whose Properties rows it could change ([`crate::props::touched`]); `None`: any.
    pub props: Option<BTreeSet<ecad_model::ObjectRef>>,
    /// Whether it could change the Pages tree ([`tree_touched`]).
    pub tree: bool,
}

/// Whether `ops` could change the Pages tree (design 019 §9): pages added, removed, renamed or
/// moved, a page's properties (number, folder), folder ops. Every other op changes nothing there.
pub fn tree_touched(ops: &[Op]) -> bool {
    ops.iter().any(|op| {
        matches!(op, Op::AddPage { .. } | Op::RemovePage { .. } | Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. })
            || matches!(op, Op::SetProperty { object: ecad_model::ObjectRef::Page(_), key, .. }
            if key == ecad_model::book::NUMBER_KEY || key == ecad_model::book::FOLDER_KEY)
    })
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
            MemoryStore::Written(f) => f.save(ledger).map_err(|e| e.to_string()).map(|_: SaveOutcome| ledger.persisted()),
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
    /// What opening reported, when the project was opened from the in-memory test store
    /// (version 1 format).
    pub open_report: Option<OpenReport>,
    /// What opening reported, when the project was opened from a file on disk (version 2).
    pub open_report_v2: Option<ecad_file::v2::ReportV2>,
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
    /// Closing the page window leaves the project open with no active page (design 016 §4.1)
    /// instead of closing the project. Off by default (tests, MCP); the trial turns it on and off
    /// again while it closes the project or quits, so unsaved work is always asked about.
    pub keep_open: bool,
    /// The page shown in the page window, if any.
    active: Option<PageId>,
    /// Each page's views as the user left them, for this session only (design 016 §4.1).
    page_views: HashMap<PageId, Vec<(cadcraft_engine::doc::Space, cadcraft_engine::View)>>,
    /// The last [`CHANGE_LOG`] changes, oldest first.
    changes: std::collections::VecDeque<Change>,
    /// Number of the last change (0: none yet).
    change_count: u64,
    /// The highest number each tag prefix has had (brief 018), read from the log as it grows.
    pub(crate) tag_high: crate::devices::TagHigh,
    /// Bumped whenever a page row could differ ([`Project::page_list_version`]).
    page_list_version: u64,
    /// What OOPS may bring back on each page document (brief 023, [`crate::oops`]).
    oops: crate::oops::Records,
    /// What the last placement in this session placed (symbol key, device), for the prompts'
    /// defaults (design 018 §5.3).
    pub(crate) last_placed: Option<(String, ComponentId)>,
    /// The last continuation name placed in this session, the prompt's default (design 022 §3.2).
    pub(crate) last_continuation: Option<String>,
    /// The Wire tool's potential for new wires in this session; `None` is `auto` (design 022 §6).
    pub wire_potential: Option<String>,
    /// The Checks list (design 022 §8), built on its first show and then kept from each change's
    /// ops ([`crate::checks::Checks::update`]); `None` until first shown.
    pub checks: Option<crate::checks::Checks>,
    /// The report models (brief 024, [`crate::outputs`]): built on first show (Reports, a BOM
    /// page, Checks) and then kept from each change's ops; `None` until then.
    pub outputs: Option<crate::outputs::Outputs>,
    /// `reports` asked for the Reports window (the trial opens it and clears this).
    pub open_reports: bool,
    /// The Checks tab's go-to ring (design 022 §8): page, point and the row's severity, until the
    /// next change. Display only, never recorded.
    pub goto_ring: Option<(PageId, Pt, crate::checks::Severity)>,
    /// The active page's frame as last drawn (brief 019): page, what it showed, its parts. Kept
    /// until an op of the frame's update map ([`crate::frame::touches`]) or another page.
    frame: Option<(PageId, crate::frame::FrameInput, Arc<Vec<crate::frame::Part>>)>,
    /// An op of the update map was committed since the frame was drawn.
    frame_stale: bool,
    /// How many times a frame was worked out again (tests and bench: only when it can differ).
    pub frame_builds: u64,
    /// The revision opened read-only (design 019 §7.3): session only, never recorded.
    pub revision_view: Option<crate::sheet_cmds::RevisionView>,
    /// Plain `u` across an issued revision stops and asks (owner answer Q7): set by the trial's
    /// confirmation, used by the next undo only.
    pub confirm_issue_undo: bool,
    /// The question an undo across an issue is waiting on, for the trial's popup.
    pub issue_undo_question: Option<String>,
    /// Plain `u` across a merge stops and asks (design 020 §4.3): set by `undomerge`, used by
    /// the next undo only.
    pub confirm_merge_undo: bool,
    /// The question an undo across a merge is waiting on, for the trial's popup.
    pub merge_undo_question: Option<String>,
    /// The merge last prepared (its states and scope), kept while nothing it read changes
    /// (brief 020b).
    pub merge_cache: Option<Arc<crate::merging::Prepared>>,
    /// Counts branch actions (create, rename, switch, archive, reopen), however they were run:
    /// what shows the branch (title, bar, Revisions, History) refreshes when it moves (brief 020a).
    branch_gen: u64,
}

/// A new project's first page: titled and numbered `1`.
fn first_page() -> Page {
    let mut p = Page::new("1");
    p.properties.insert(ecad_model::book::NUMBER_KEY.into(), "1".into());
    p
}

/// What this program records about itself in a file header and a writer lock.
pub fn writer() -> String {
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
        Shape::Path { points, closed } => kind_for_primitive(&Primitive::Polyline { vertices: points.clone(), closed: *closed }),
        Shape::Arc { center, radius, start, end } => {
            kind_for_primitive(&Primitive::Arc { center: *center, radius: *radius, start: *start, end: *end })
        }
        Shape::Text { at, rotation, value } => {
            kind_for_primitive(&Primitive::Text { at: *at, height: LABEL_HEIGHT, rotation: *rotation, value: value.clone() })
        }
    };
    let lineweight = match part.weight {
        Weight::Cable => Lineweight::Mm100(crate::look::CABLE_WEIGHT),
        Weight::Wire => Lineweight::Mm100(crate::look::WIRE_WEIGHT),
        Weight::Text => Lineweight::ByLayer,
    };
    let color = part.color.map_or(cadcraft_engine::doc::color::Color::ByLayer, cadcraft_engine::doc::color::Color::Index);
    Entity { handle, common: Common { layer: crate::views::LAYER.into(), lineweight, color, ..Common::default() }, kind }
}

pub(crate) fn base_drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    if d.layer(MARKER_LAYER).is_none() {
        let mut l = CLayer::new(MARKER_LAYER);
        l.plot = false;
        l.color = cadcraft_engine::doc::color::Color::Index(8);
        d.layers.push(l);
    }
    // The starter symbols' terminals lie on a 5 mm grid (design 018 §4, `ecad.symbol-grid`): snap
    // and grid spacing of an ECAD page follow it.
    let grid =
        cadcraft_engine::doc::HVal::Point(cadcraft_engine::geom::Vec3::new(crate::symbols::SYMBOL_GRID_MM, crate::symbols::SYMBOL_GRID_MM, 0.0));
    d.header.set("SNAPUNIT", grid.clone());
    d.header.set("GRIDUNIT", grid);
    d
}

/// The label an unavailable page shows instead of its drawing.
pub const PLACEHOLDER_TEXT: &str =
    "PAGE UNAVAILABLE: its drawing could not be rebuilt. The project is intact and can be saved. Close this page and reopen it.";
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
    store.push(Entity { handle, common: Common { layer: "0".into(), ..Common::default() }, kind: EntityKind::Text(text) });
    d.model = store;
    d
}

/// Project one page of `doc` onto `base` (a drawing whose tables and header are kept), reusing
/// handles from `prev` for objects that are still there.
///
/// # Errors
/// An item that cannot be drawn (missing layer or definition).
pub fn project_page(doc: &Document, page: PageId, base: &Drawing, prev: &HandleMap) -> Result<(Drawing, HandleMap), String> {
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
    doc: &Document, page: PageId, base: &Drawing, prev: &HandleMap, fan: &BTreeSet<CableId>, tol: f64,
) -> Result<(Drawing, HandleMap), String> {
    project_page_framed(doc, page, base, prev, fan, tol, &crate::generated::parts_of(doc, page, None))
}

/// [`project_page_with`] with the sheet frame's parts already worked out (the project keeps them
/// between edits that do not change them, design 019 §9).
///
/// # Errors
/// As [`project_page`].
pub fn project_page_framed(
    doc: &Document, page: PageId, base: &Drawing, prev: &HandleMap, fan: &BTreeSet<CableId>, tol: f64, frame: &[crate::frame::Part],
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
    if d.layer(crate::devices::TAG_LAYER).is_none() {
        d.layers.push(CLayer::new(crate::devices::TAG_LAYER));
    }
    if d.layer(crate::wires::LABEL_LAYER).is_none() {
        let mut l = CLayer::new(crate::wires::LABEL_LAYER);
        l.color = cadcraft_engine::doc::color::Color::Index(crate::wires::LABEL_ACI);
        d.layers.push(l);
    }
    if d.layer(crate::frame::LAYER).is_none() {
        d.layers.push(crate::frame::layer());
    }
    crate::locations::add_linetype(&mut d);
    if d.layer(crate::generated::LAYER).is_none() {
        let mut l = CLayer::new(crate::generated::LAYER);
        l.locked = true;
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
    d.blocks.retain(|name, _| !name.starts_with(ECAD_PREFIX) || crate::convert::definition_of_block(name).is_some_and(|id| used.contains(&id)));
    for def_id in &used {
        let name = block_name(*def_id);
        if d.blocks.contains_key(&name) {
            continue;
        }
        let def = doc.definitions.get(def_id).ok_or_else(|| format!("definition {def_id} is missing"))?;
        let mut b = Block::new(&name);
        for g in &def.graphics {
            let h = d.new_handle();
            let common = cadcraft_engine::doc::Common { layer: "0".into(), color: cadcraft_engine::doc::color::Color::ByBlock, ..Default::default() };
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
            groups.push(Group { name: format!("{ECAD_PREFIX}{:032x}", rid.0), description: String::new(), selectable: true, members });
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
    // Device tags and terminal labels (brief 018): derived on every draw, never stored.
    for t in crate::devices::texts_on_page(doc, page) {
        let h = prev.tag_text(t.rep, t.of).unwrap_or_else(|| d.new_handle());
        ents.push(crate::devices::text_entity(&t, h));
        map.insert_tag_text(t.rep, t.of, h);
    }
    // Location box labels (brief 024): derived on every draw, never stored.
    for (rep, value, at) in crate::locations::labels_on_page(doc, page) {
        let h = prev.tag_text(rep, crate::devices::TextOf::Tag).unwrap_or_else(|| d.new_handle());
        ents.push(crate::locations::label_entity(&value, at, h));
        map.insert_tag_text(rep, crate::devices::TextOf::Tag, h);
    }
    // Conductor labels (brief 022): derived on every draw of the page, never stored.
    // A cable in fan view hides its members' routes, and their labels with them (owner,
    // 2026-11-09): display only, nothing is written.
    for l in crate::wires::labels_on_page_with(doc, page, &hidden) {
        let h = prev.label(l.rep).unwrap_or_else(|| d.new_handle());
        ents.push(crate::wires::label_entity(&l, h));
        map.insert_label(l.rep, l.conductor, h);
    }
    ents.sort_by_key(|e| e.handle);
    let mut store = EntityStore::new();
    // The sheet frame and title block (brief 019): derived, never stored, and drawn first so
    // everything else lies on top of it (picks find the drawing, not the frame).
    for (i, part) in frame.iter().enumerate() {
        let index = u32::try_from(i).map_err(|_| "the frame has too many parts")?;
        let h = prev.frame_part(index).unwrap_or_else(|| d.new_handle());
        store.push(crate::frame::entity(part, h));
        map.insert_frame(index, h);
    }
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
    pub fn create_with(s: &mut Session, name: &str, seed: u64, persistence: Persistence) -> Result<ProjectHandle, String> {
        let mut ids = IdAllocator::seeded(seed);
        let mut ledger = Ledger::new(seed.rotate_left(32));
        let (page, layer): (PageId, LayerId) = (ids.fresh(), ids.fresh());
        ledger
            .commit_at(
                Author::User,
                "New project",
                vec![
                    // The first page is numbered 1 (brief 019): stored, so moving pages never
                    // renumbers it.
                    Op::AddPage { id: page, page: first_page(), index: 0 },
                    Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
                    // The length unit is recorded at creation (proposal §20.13).
                    Op::SetProjectSettings {
                        before: ProjectSettings::default(),
                        after: ProjectSettings { length_unit: LengthUnit::Millimetre, ..ProjectSettings::default() },
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
            open_report_v2: None,
            pages: BTreeMap::new(),
            events: EventLog::default(),
            clips: BTreeMap::new(),
            oops: crate::oops::Records::default(),
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
            keep_open: false,
            active: None,
            page_views: HashMap::new(),
            changes: std::collections::VecDeque::new(),
            change_count: 0,
            tag_high: crate::devices::TagHigh::default(),
            page_list_version: 0,
            last_placed: None,
            last_continuation: None,
            wire_potential: None,
            checks: None,
            outputs: None,
            open_reports: false,
            goto_ring: None,
            frame: None,
            frame_stale: false,
            frame_builds: 0,
            revision_view: None,
            confirm_issue_undo: false,
            issue_undo_question: None,
            confirm_merge_undo: false,
            merge_undo_question: None,
            merge_cache: None,
            branch_gen: 0,
        }));
        let mut p = crate::access::try_lock(&project)?;
        p.me = Arc::downgrade(&project);
        // Read the history once, so tag numbering never reads the log during a prompt.
        p.catch_up_tags();
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
    pub(crate) fn open_from(s: &mut Session, name: &str, medium: MemoryMedium, seed: u64) -> Result<ProjectHandle, String> {
        let (file, ledger, report) = ProjectFile::open(medium, seed).map_err(|e| e.to_string())?;
        let id = file.header().project_id.ok_or("the file has no project ID")?;
        let default_layer = ledger.document().layers.iter().find(|(_, l)| l.name == "0").map(|(id, _)| *id).ok_or("no layer 0")?;
        let pages: Vec<PageId> = ledger.document().page_order.clone();
        // A fresh ID stream even when the caller reuses the creation seed.
        let ids = IdAllocator::seeded_after(seed, ledger.log().last().map_or(0, |t| t.id.0) ^ u128::from(ledger.head()));
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
            open_report_v2: None,
            pages: pages.iter().map(|p| (*p, PageView::default())).collect(),
            events: EventLog::default(),
            clips: BTreeMap::new(),
            oops: crate::oops::Records::default(),
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
            keep_open: false,
            active: None,
            page_views: HashMap::new(),
            changes: std::collections::VecDeque::new(),
            change_count: 0,
            tag_high: crate::devices::TagHigh::default(),
            page_list_version: 0,
            last_placed: None,
            last_continuation: None,
            wire_potential: None,
            checks: None,
            outputs: None,
            open_reports: false,
            goto_ring: None,
            frame: None,
            frame_stale: false,
            frame_builds: 0,
            revision_view: None,
            confirm_issue_undo: false,
            issue_undo_question: None,
            confirm_merge_undo: false,
            merge_undo_question: None,
            merge_cache: None,
            branch_gen: 0,
        }));
        let mut p = crate::access::try_lock(&project)?;
        p.me = Arc::downgrade(&project);
        // Read the history once, so tag numbering never reads the log during a prompt.
        p.catch_up_tags();
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
    pub fn open_file_in(s: &mut Session, path: &str, seed: u64, settings: crate::views::Settings) -> Result<(ProjectHandle, String), String> {
        Self::open_file_with(s, path, seed, settings, false)
    }

    /// [`Project::open_file_in`], for reading only when `reading`: no writer lock is taken and
    /// the file is never changed (an agent with writes off).
    ///
    /// # Errors
    /// As [`Project::open_file`].
    pub fn open_file_with(
        s: &mut Session, path: &str, seed: u64, settings: crate::views::Settings, reading: bool,
    ) -> Result<(ProjectHandle, String), String> {
        let path = crate::file::normalize(path)?;
        let mut o =
            if reading { crate::file::Binding::open_read_only(&path, seed, &writer())? } else { crate::file::Binding::open(&path, seed, &writer())? };
        let ledger = o.ledger;
        let default_layer = ledger.document().layers.iter().find(|(_, l)| l.name == "0").map(|(id, _)| *id).ok_or("no layer 0")?;
        let pages: Vec<PageId> = ledger.document().page_order.clone();
        // A fresh ID stream for this session, never reissuing the file's IDs.
        let ids = IdAllocator::seeded_after(seed, ledger.log().last().map_or(0, |t| t.id.0) ^ u128::from(ledger.head()));
        let baseline = ledger.head();
        let mut summary = vec![format!("Opened {path} at transaction {}.", ledger.head())];
        if o.report.damage.is_some() || o.report.repaired {
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
        let views: BTreeSet<CableId> = loaded.fan.into_iter().filter(|k| ledger.document().cables.contains_key(k)).collect();
        let summary = summary.join("\n");
        let project = Arc::new(Mutex::new(Project {
            name: file_stem(&path),
            path: Some(path.clone()),
            ledger,
            ids,
            id: o.project,
            store: MemoryStore::default(),
            file: o.binding,
            open_report: None,
            open_report_v2: Some(o.report),
            pages: pages.iter().map(|p| (*p, PageView::default())).collect(),
            events: EventLog::default(),
            clips: BTreeMap::new(),
            oops: crate::oops::Records::default(),
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
            keep_open: false,
            active: None,
            page_views: HashMap::new(),
            changes: std::collections::VecDeque::new(),
            change_count: 0,
            tag_high: crate::devices::TagHigh::starting_at(std::mem::take(&mut o.tag_high)),
            page_list_version: 0,
            last_placed: None,
            last_continuation: None,
            wire_potential: None,
            checks: None,
            outputs: None,
            open_reports: false,
            goto_ring: None,
            frame: None,
            frame_stale: false,
            frame_builds: 0,
            revision_view: None,
            confirm_issue_undo: false,
            issue_undo_question: None,
            confirm_merge_undo: false,
            merge_undo_question: None,
            merge_cache: None,
            branch_gen: 0,
        }));
        let mut p = crate::access::try_lock(&project)?;
        p.me = Arc::downgrade(&project);
        // Read the history once, so tag numbering never reads the log during a prompt.
        p.catch_up_tags();
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

    /// Save the pending transactions to the bound file (Save, as the branch actions of brief 020
    /// need before they write: owner answer 3, "Save and switch").
    ///
    /// # Errors
    /// No file, or the save failed (in words).
    pub(crate) fn save_pending(&mut self, pg: &mut Pages<'_>) -> Result<(), String> {
        if !self.unsaved() {
            return Ok(());
        }
        self.save_file(pg, None, false).map(|_| ()).map_err(|e| e.to_string())
    }

    /// Show another branch's state (design 020 §3.3): `ledger` replaces the project's, every view
    /// rebuilds once as on open, the active page stays if it exists on the new branch (else the
    /// first page in book order), and a read-only point view closes. Not a step.
    pub(crate) fn adopt_branch(&mut self, pg: &mut Pages<'_>, ledger: Ledger, tag_high: ecad_model::tag::HighWater) -> Result<(), String> {
        let seed = session_seed();
        let window = self.window();
        let doc = ledger.document();
        let target = self.active.filter(|p| doc.pages.contains_key(p)).or_else(|| ecad_model::book::Book::of(doc).order.first().copied());
        self.ids = IdAllocator::seeded_after(seed, ledger.log().last().map_or(0, |t| t.id.0) ^ u128::from(ledger.head()));
        self.baseline = ledger.head();
        let pages: Vec<PageId> = doc.page_order.clone();
        let cables: BTreeSet<CableId> = doc.cables.keys().copied().collect();
        self.ledger = ledger;
        self.tag_high = crate::devices::TagHigh::starting_at(tag_high);
        self.catch_up_tags();
        self.views.retain(|c| cables.contains(c));
        self.checks = None;
        self.outputs = None;
        self.goto_ring = None;
        self.frame = None;
        self.frame_stale = false;
        self.revision_view = None;
        self.clips.clear();
        self.last_placed = None;
        // Every change-driven copy (pages, devices, properties, checks) rebuilds once.
        self.changes.clear();
        self.change_count += 2;
        self.page_list_version += 1;
        let mut views: BTreeMap<PageId, PageView> = pages.iter().map(|p| (*p, PageView::default())).collect();
        if let Some((old, uid)) = window {
            self.oops.stale_doc(uid);
            if let Some(st) = pg.doc_mut(uid) {
                st.selection.clear();
                st.previous_selection.clear();
                st.undo.clear();
                st.redo.clear();
            }
            match target {
                Some(t) if t == old => {
                    // The same page: projected again, with fresh handles, in the same window.
                    views.insert(old, PageView { uid: Some(uid), ..PageView::default() });
                    self.pages = views;
                    if let Some(st) = pg.doc_mut(uid) {
                        st.doc = Arc::new(base_drawing());
                    }
                    self.refresh_page(pg, old, None, None, true);
                    if let Some(st) = pg.doc_mut(uid) {
                        st.title = self.page_title(old);
                    }
                }
                Some(t) => {
                    // Another page, shown in the same window; the old page's entry goes if the
                    // branch has no such page.
                    views.insert(old, PageView { uid: Some(uid), ..PageView::default() });
                    self.pages = views;
                    self.open_page(pg, t)?;
                    if !self.ledger.document().pages.contains_key(&old) {
                        self.pages.remove(&old);
                    }
                }
                None => {
                    self.pages = views;
                    self.active = None;
                }
            }
        } else {
            self.pages = views;
        }
        self.sync_saved(pg);
        let head = self.ledger.head();
        self.events.record(EventKind::ProjectOpened, Some("switchbranch"), head, None, "another branch shown");
        self.branch_gen += 1;
        Ok(())
    }

    /// How many branch actions this session has seen ([`Project`]'s `branch_gen`).
    pub fn branch_gen(&self) -> u64 {
        self.branch_gen
    }

    /// A branch action happened (brief 020a): every view of the branches refreshes.
    pub(crate) fn note_branch_change(&mut self) {
        self.branch_gen += 1;
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
                self.events.record(EventKind::Saved, Some(what), head, None, format!("{shown} through transaction {n}"));
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

    /// Project `page` with this project's cable views and its frame (worked out again only when
    /// the frame's update map says it may differ, or for another page).
    fn project(&mut self, page: PageId, base: &Drawing, prev: &HandleMap) -> Result<(Drawing, HandleMap), String> {
        let parts = self.frame_parts(page);
        project_page_framed(self.ledger.document(), page, base, prev, &self.views, self.tolerance, &parts)
    }

    /// The frame parts of `page`, from the kept copy when nothing it shows can have changed.
    fn frame_parts(&mut self, page: PageId) -> Arc<Vec<crate::frame::Part>> {
        if let Some((p, _, parts)) = &self.frame
            && *p == page
            && !self.frame_stale
        {
            return parts.clone();
        }
        // A BOM page's sheet count (and so every page's Total) needs the report models (brief 024).
        if crate::generated::has_bom_page(self.ledger.document()) {
            self.outputs();
        }
        let doc = self.ledger.document();
        let (input, total) = crate::generated::input_of(doc, page, self.outputs.as_ref());
        self.frame_stale = false;
        let generated = crate::generated::kind_of(doc, page).is_some();
        if let Some((p, old, parts)) = &self.frame
            && *p == page
            && *old == input
            && !generated
        {
            return parts.clone();
        }
        self.frame_builds += 1;
        let parts = Arc::new(crate::generated::page_parts(doc, page, &input, self.outputs.as_ref(), total));
        self.frame = Some((page, input, parts.clone()));
        parts
    }

    /// Mark the kept frame stale when `ops` (about to be committed) can change it.
    pub(crate) fn note_frame_ops(&mut self, ops: &[Op]) {
        if let Some((p, input, _)) = &self.frame
            && crate::frame::touches(ops, *p, input)
        {
            self.frame_stale = true;
        }
        // A generated page's content (brief 024) has its own map.
        if let Some((p, _, _)) = &self.frame
            && let Some(kind) = crate::generated::kind_of(self.ledger.document(), *p)
            && crate::generated::touches(ops, kind)
        {
            self.frame_stale = true;
        }
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
        // A view is drawn on every page showing the cable: the active page is redrawn, every
        // other page when it becomes active.
        self.record_change(None, crate::device_list::DeviceTouch::default(), Some(BTreeSet::new()), false);
        self.redraw_window(pg, Some("cableview"));
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

    /// The page window's title: `demo - page 2: Field`.
    fn page_title(&self, page: PageId) -> String {
        let doc = self.ledger.document();
        let t = doc.pages.get(&page).map(|p| p.title.clone()).unwrap_or_default();
        format!("{} - page {}: {t}", self.name, crate::references::page_number(doc, page))
    }

    /// The page shown in the page window (the active page), if the window is open.
    pub fn active_page(&self) -> Option<PageId> {
        self.active.filter(|p| self.pages.get(p).is_some_and(|v| v.uid.is_some()))
    }

    /// The page window: the active page and the engine document showing it.
    pub fn window(&self) -> Option<(PageId, u64)> {
        let page = self.active_page()?;
        Some((page, self.pages.get(&page)?.uid?))
    }

    /// The Project panel's page rows (brief 016): every page in book order (brief 019) with the
    /// number it is shown with and its title. Reads pages and folders only: O(pages + folders),
    /// never O(items).
    pub fn page_rows(&self) -> Vec<PageRow> {
        self.tree_rows()
            .into_iter()
            .filter_map(|r| match r {
                TreeRow::Page(p) => Some(p),
                TreeRow::Folder { .. } => None,
            })
            .collect()
    }

    /// The Pages tree (design 019 §5): folders and pages in book order, each with its depth.
    /// Reads pages and folders only: O(pages + folders).
    pub fn tree_rows(&self) -> Vec<TreeRow> {
        tree_rows(self.ledger.document(), |id| self.pages.get(&id).is_some_and(|v| v.unavailable))
    }

    /// A number that changes exactly when [`Project::page_rows`] could differ (update map of the
    /// Project panel's rows, brief 016): a page added or removed (rename and move are a remove and
    /// an add, [`crate::device_list::DeviceTouch::pages`]), or a page becoming available or
    /// unavailable. Every other change (drawing edits, devices, undo or redo of those, view
    /// switches) leaves it alone, so the panel keeps its rows.
    pub fn page_list_version(&self) -> u64 {
        self.page_list_version
    }

    /// Number of the last change the project went through (a transaction, an undo or redo, a view
    /// switch); 0 before the first. Cheap: callers compare it every frame.
    pub fn change_count(&self) -> u64 {
        self.change_count
    }

    /// The pages whose drawing the changes after change `n` could have changed: `Some(empty)`
    /// when none, `None` when any page could have changed or the changes are too old to tell.
    pub fn changes_since(&self, n: u64) -> Option<BTreeSet<PageId>> {
        if n >= self.change_count {
            return Some(BTreeSet::new());
        }
        if self.changes.front().is_none_or(|c| c.n > n + 1) {
            return None;
        }
        let mut out = BTreeSet::new();
        for c in self.changes.iter().filter(|c| c.n > n) {
            out.extend(c.pages.as_ref()?.iter().copied());
        }
        Some(out)
    }

    /// What the changes after change `n` could change in the Devices list, together; `None` when
    /// the changes are too old to tell (rebuild the list).
    pub fn device_changes_since(&self, n: u64) -> Option<crate::device_list::DeviceTouch> {
        let mut out = crate::device_list::DeviceTouch::default();
        if n >= self.change_count {
            return Some(out);
        }
        if self.changes.front().is_none_or(|c| c.n > n + 1) {
            return None;
        }
        for c in self.changes.iter().filter(|c| c.n > n) {
            out.components.extend(c.devices.components.iter().copied());
            out.pages |= c.devices.pages;
            out.settings |= c.devices.settings;
        }
        Some(out)
    }

    /// The objects whose Properties rows the changes after change `n` could have changed, together;
    /// `None` when any could have, or the changes are too old to tell (rebuild the copy).
    pub fn props_changes_since(&self, n: u64) -> Option<BTreeSet<ecad_model::ObjectRef>> {
        if n >= self.change_count {
            return Some(BTreeSet::new());
        }
        if self.changes.front().is_none_or(|c| c.n > n + 1) {
            return None;
        }
        let mut out = BTreeSet::new();
        for c in self.changes.iter().filter(|c| c.n > n) {
            out.extend(c.props.as_ref()?.iter().copied());
        }
        Some(out)
    }

    /// Bring the tag numbering up to date with the log.
    fn catch_up_tags(&mut self) {
        self.tag_high.catch_up(&self.ledger);
    }

    /// Record one change and the pages it could change.
    ///
    /// The tag numbering ([`crate::devices::TagHigh`]) reads the transactions this change added,
    /// so reads (prompts, previews, pick lists) never read the log. The Pages tree's version moves
    /// only with a change of its update map (`tree`, brief 019, or pages added or removed).
    fn record_change(
        &mut self, pages: Option<BTreeSet<PageId>>, devices: crate::device_list::DeviceTouch, props: Option<BTreeSet<ecad_model::ObjectRef>>,
        tree: bool,
    ) {
        self.goto_ring = None;
        self.tag_high.catch_up(&self.ledger);
        if devices.pages || tree {
            self.page_list_version += 1;
        }
        self.change_count += 1;
        self.changes.push_back(Change { n: self.change_count, pages, devices, props, tree });
        while self.changes.len() > CHANGE_LOG {
            self.changes.pop_front();
        }
    }

    /// Make `page` the active page (brief 016): it is projected into the project's one page window,
    /// which is opened if it is closed. The page shown before keeps its view for this session; a
    /// page shown before gets its view back, one shown for the first time is zoomed to its
    /// extents. A page already active is only brought to the front.
    ///
    /// # Errors
    /// Unknown page or projection failure; the page shown before stays shown.
    pub(crate) fn open_page(&mut self, pg: &mut Pages<'_>, page: PageId) -> Result<u64, String> {
        if !self.ledger.document().pages.contains_key(&page) {
            return Err(format!("page {page} does not exist"));
        }
        if let Some(uid) = self.pages.get(&page).and_then(|v| v.uid)
            && pg.activate(uid)
        {
            return Ok(uid);
        }
        let (d, map) = self.project(page, &base_drawing(), &HandleMap::default())?;
        let empty = d.model.is_empty();
        let title = self.page_title(page);
        let window = self.window().filter(|(_, uid)| pg.doc(*uid).is_some());
        let uid = match window {
            Some((old, uid)) => {
                let Some(st) = pg.doc_mut(uid) else { return Err("no document".into()) };
                let left = std::mem::take(&mut st.views);
                st.doc = Arc::new(d);
                st.title = title.clone();
                st.selection.clear();
                st.previous_selection.clear();
                st.space = cadcraft_engine::doc::Space::Model;
                st.mspace = None;
                st.view_history.clear();
                st.undo.clear();
                st.redo.clear();
                st.block_edit = None;
                st.revision += 1;
                self.page_views.insert(old, left);
                // What OOPS kept belongs to the page drawn there before (brief 023).
                self.oops.stale_doc(uid);
                if let Some(v) = self.pages.get_mut(&old) {
                    *v = PageView::default();
                }
                pg.activate(uid);
                uid
            }
            None => {
                let owner = self.owner_ref().ok_or("project is not shared")?;
                pg.open(d, &title, self.path.clone(), owner).ok_or("no document")?
            }
        };
        let remembered = self.page_views.get(&page).cloned();
        let has_view = remembered.is_some();
        let last = match pg.doc_mut(uid) {
            Some(st) => {
                if let Some(views) = remembered {
                    st.views = views;
                }
                Some(st.doc.clone())
            }
            None => None,
        };
        if !has_view && !empty {
            pg.zoom_extents();
        }
        let was = self.pages.insert(page, PageView { uid: Some(uid), map, last, unavailable: false });
        if was.is_some_and(|v| v.unavailable) {
            self.page_list_version += 1;
        }
        self.active = Some(page);
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
        let pages: Vec<PageId> = self.pages.iter().filter(|(_, v)| v.uid.is_some() && !v.unavailable).map(|(p, _)| *p).collect();
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
        &mut self, pg: &mut Pages<'_>, page: PageId, command: Option<&str>, committed: Option<(&Arc<Drawing>, &BTreeSet<Handle>)>, recovery: bool,
    ) {
        let Some(view) = self.pages.get(&page) else { return };
        let Some(uid) = view.uid else { return };
        let Some(current) = pg.doc(uid).map(|st| st.doc.clone()) else { return };
        let current = Some(current);
        let base = committed.map(|(d, _)| d.clone()).or(current).unwrap_or_else(|| Arc::new(base_drawing()));
        let prev = view.map.clone();
        let head = self.ledger.head();
        let first = if self.take_projection_fault() { Err("injected projection failure".to_string()) } else { self.project(page, &base, &prev) };
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
                    Ok(_) => self.events.record(EventKind::ProjectionRebuilt, command, head, Some(page), "drawing and map rebuilt from the document"),
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
                    self.events.record(EventKind::PageUnavailable, command, head, Some(page), "rebuilt page did not verify");
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
                    // Tag texts and terminal labels follow their symbols (brief 018).
                    amended.extend(prev.tag_text_handles().chain(map.tag_text_handles()));
                    // Conductor labels follow their routes and nets (brief 022).
                    amended.extend(prev.label_handles().chain(map.label_handles()));
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
                let changed = st.doc.model != d.model || st.doc.groups != d.groups || st.doc.blocks != d.blocks || st.doc.layers != d.layers;
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
                    if v.unavailable {
                        v.unavailable = false;
                        self.page_list_version += 1;
                    }
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
                    if !v.unavailable {
                        v.unavailable = true;
                        self.page_list_version += 1;
                    }
                }
                pg.echo("This page could not be drawn; it is unavailable until it is closed and reopened. The project is intact and can be saved.");
            }
        }
    }

    /// After a transaction that is not a drawing commit (an ECAD command's JSON form, undo, redo)
    /// or a view switch: redraw the active page, the only page drawn (brief 016). Other pages are
    /// projected when they become active, so nothing else is marked or redrawn.
    pub(crate) fn redraw_window(&mut self, pg: &mut Pages<'_>, command: Option<&str>) {
        if let Some(page) = self.active_page().filter(|p| self.pages.get(p).is_some_and(|v| !v.unavailable)) {
            self.refresh_page(pg, page, command, None, false);
        }
        self.sync_saved(pg);
    }

    /// Commit ECAD-made ops (Path A) and refresh the open pages.
    ///
    /// # Errors
    /// Validation failure; nothing changes.
    pub(crate) fn apply_edit(&mut self, pg: &mut Pages<'_>, command: &str, label: &str, ops: Vec<Op>) -> Result<ecad_model::TxId, String> {
        self.apply_edit_with(pg, command, label, ops, true)
    }

    /// [`Project::apply_edit`], with the numbering pass (brief 022) only when `numbering`: a
    /// Restore (brief 020) records exactly the state it returns to, wire numbers included.
    pub(crate) fn apply_edit_with(
        &mut self, pg: &mut Pages<'_>, command: &str, label: &str, ops: Vec<Op>, numbering: bool,
    ) -> Result<ecad_model::TxId, String> {
        self.apply_tx(pg, command, label, ops, numbering, None)
    }

    /// Commit a merge (brief 020b): `ops` as one transaction of `kind` (a merge), updated in the
    /// views as any commit (their update maps; no special path).
    ///
    /// # Errors
    /// As [`Project::apply_edit`].
    pub(crate) fn apply_merge(
        &mut self, pg: &mut Pages<'_>, label: &str, kind: ecad_model::TxKind, ops: Vec<Op>,
    ) -> Result<ecad_model::TxId, String> {
        self.apply_tx(pg, "merge", label, ops, false, Some(kind))
    }

    fn apply_tx(
        &mut self, pg: &mut Pages<'_>, command: &str, label: &str, ops: Vec<Op>, numbering: bool, kind: Option<ecad_model::TxKind>,
    ) -> Result<ecad_model::TxId, String> {
        if pg.command_running() {
            return Err("finish or cancel the current command first (TX-05)".into());
        }
        // Nothing is drawn on a generated page (brief 024, design 024 §5.3).
        if let Some(m) = crate::generated::refusal(self.ledger.document(), &ops) {
            return Err(m);
        }
        #[cfg(feature = "fault-injection")]
        if std::mem::take(&mut self.faults.panic_in_command) {
            crate::fault_panic();
        }
        // References are derived for the cables the ops could touch only (brief 014).
        let scope = crate::references::cables_touched(self.ledger.document(), &ops);
        let pages = pages_touched(self.ledger.document(), &ops);
        let devices = crate::device_list::devices_touched(self.ledger.document(), &ops);
        let props = crate::props::touched(self.ledger.document(), &ops);
        let tree = tree_touched(&ops);
        let refs_before = crate::references::Snapshot::take(self.ledger.document(), scope.as_ref());
        let checks = self.checks.as_ref().map(|c| c.scope(self.ledger.document(), &ops));
        let outputs = self.outputs.as_ref().map(|o| o.scope(self.ledger.document(), &ops));
        let n = ops.len();
        let frame_before = self.frame_stale;
        self.note_frame_ops(&ops);
        // The numbering pass (brief 022) extends the same transaction.
        let pass = |d: &ecad_model::Document, o: &[Op]| if numbering { ecad_model::wiring::assign(d, o) } else { Vec::new() };
        let committed = match kind {
            Some(k) => self.ledger.commit_merge(self.author.clone(), label, k, ops, now_ms()),
            None => self.ledger.commit_extended(self.author.clone(), label, ops, now_ms(), pass),
        };
        let tx = committed.map_err(|e| {
            self.frame_stale = frame_before;
            self.events.record(EventKind::Refused, Some(command), self.ledger.head(), None, e.to_string());
            e.to_string()
        })?;
        self.events.record(EventKind::Accepted, Some(command), self.ledger.head(), None, label);
        let refs_after = crate::references::Snapshot::take(self.ledger.document(), scope.as_ref());
        let numbered = self.numbered_pages(n);
        let props = self.with_numbered(props, n);
        self.update_checks(checks);
        self.update_outputs(outputs);
        self.record_change(touched(touched(pages, refs_before.pages_changed(&refs_after)), numbered), devices, props, tree);
        self.redraw_window(pg, Some(command));
        if let Some(line) = refs_before.orphaned_line(self.ledger.document(), &refs_after) {
            pg.echo(line);
        }
        Ok(tx)
    }

    /// Bring the kept Checks list (if shown once) in line with the last transaction, from its
    /// ops only (design 022 §11); `scope` was worked out before it.
    fn update_checks(&mut self, scope: Option<crate::checks::Scope>) {
        let (Some(scope), Some(mut checks)) = (scope, self.checks.take()) else { return };
        let ledger = &self.ledger;
        let ops = ledger.log().last().map(|t| t.ops.as_slice()).unwrap_or_default();
        checks.update(ledger.document(), ops, scope, self.tolerance);
        self.checks = Some(checks);
    }

    /// Bring the kept report models (if shown once) in line with the last transaction, from its
    /// ops only (design 024 §9); `scope` was worked out before it. A change of a generated page's
    /// sheet count marks the frame (its Total) stale.
    fn update_outputs(&mut self, scope: Option<crate::outputs::Scope>) {
        let (Some(scope), Some(mut o)) = (scope, self.outputs.take()) else { return };
        let ledger = &self.ledger;
        let ops = ledger.log().last().map(|t| t.ops.as_slice()).unwrap_or_default();
        let (before, revision) = (crate::generated::bom_sheets(&o), o.revision);
        o.update(ledger.document(), ops, scope);
        let bom_shown = self
            .frame
            .as_ref()
            .is_some_and(|(p, _, _)| crate::generated::kind_of(ledger.document(), *p) == Some(crate::generated::GenKind::BomLocation));
        if crate::generated::bom_sheets(&o) != before || (bom_shown && o.revision != revision) {
            self.frame_stale = true;
        }
        self.outputs = Some(o);
    }

    /// The report models, built whole on their first show (design 024 §8.1) and kept after that.
    pub fn outputs(&mut self) -> &crate::outputs::Outputs {
        let doc = self.ledger.document();
        self.outputs.get_or_insert_with(|| crate::outputs::Outputs::build(doc))
    }

    /// The Checks list, built whole on its first show (design 022 §8) and kept after that.
    pub fn checks(&mut self) -> &crate::checks::Checks {
        let doc = self.ledger.document();
        let tol = self.tolerance;
        self.checks.get_or_insert_with(|| crate::checks::Checks::build(doc, tol))
    }

    /// `props` with the conductors the numbering pass of the last transaction renumbered (its ops
    /// after the first `n`): the Properties view of a wire reads its number (brief 022).
    fn with_numbered(&self, props: Option<BTreeSet<ecad_model::ObjectRef>>, n: usize) -> Option<BTreeSet<ecad_model::ObjectRef>> {
        let mut props = props?;
        let extra = self.ledger.log().last().map(|t| t.ops.get(n..).unwrap_or_default()).unwrap_or_default();
        for op in extra {
            if let Op::SetProperty { object, .. } = op {
                props.insert(*object);
            }
        }
        Some(props)
    }

    /// The pages whose labels the numbering pass of the last transaction changed: the pages of
    /// the conductors its ops (after the first `n`, the command's own) renumbered (brief 022).
    fn numbered_pages(&self, n: usize) -> Option<BTreeSet<PageId>> {
        let doc = self.ledger.document();
        let extra = self.ledger.log().last().map(|t| t.ops.get(n..).unwrap_or_default()).unwrap_or_default();
        let mut out = BTreeSet::new();
        for op in extra {
            if let Op::SetProperty { object: ecad_model::ObjectRef::Conductor(c), .. } = op {
                out.extend(doc.representations_of(Subject::Conductor(*c)).map(|(_, r)| r.page));
            }
        }
        Some(out)
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
        LiveFacts { pages, upstream: Some(crate::UPSTREAM_COMMIT.into()), persistence: Some(self.file_facts().to_json()) }
    }

    /// A JSON snapshot (live when `at` is `None`).
    ///
    /// # Errors
    /// See [`ecad_inspect::snapshot`].
    pub fn snapshot(&self, pg: &PagesRef<'_>, at: Option<u64>) -> Result<Value, String> {
        let facts = self.live_facts(pg);
        ecad_inspect::snapshot(&self.name, &self.ledger, at, &self.events, at.is_none().then_some(&facts)).map_err(|e| e.to_string())
    }

    /// Positions of attached route ends that follow the dragged terminals (drag preview, M2).
    fn following_routes(&self, page: PageId, moved: &dyn Fn(TerminalId) -> Option<Pt>) -> Vec<cadcraft_engine::doc::EntityKind> {
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
    let mut out: Vec<Handle> = a.iter().filter(|(h, e)| !amended.contains(h) && b.get(h).is_none_or(|x| x != *e)).map(|(h, _)| *h).collect();
    out.extend(b.keys().filter(|h| !a.contains_key(h) && !amended.contains(h)));
    out.sort();
    out
}

/// Commands refused on ECAD pages before they start (owner decision, 2026-10-31, brief 007): the
/// Block Editor and in-place reference editing swap or rewrite the page drawing and its blocks
/// outside the commit gate, and APPLOAD and LISP evaluate LISP (see [`LISP_REFUSED`]).
pub const REFUSED_COMMANDS: &[&str] = &["bedit", "bsave", "bsaveas", "bclose", "refedit", "refset", "refclose", "appload", "lisp"];

/// Why LISP is refused on ECAD pages: its built-ins change the drawing outside any command, and a
/// routine keeps an undo step of its own.
pub const LISP_REFUSED: &str = "LISP is not available on ECAD pages: it changes the drawing outside the commands ECAD records";

impl DocumentOwner for Project {
    fn refuse_start(&self, _s: &Session, uid: u64, start: Start<'_>) -> Option<String> {
        self.page_of(uid)?;
        match start {
            Start::Lisp => Some(LISP_REFUSED.into()),
            Start::Command("appload" | "lisp") => Some(LISP_REFUSED.into()),
            Start::Command("oops") => self.page_of(uid).and_then(|page| self.oops.refuse_start(uid, page)),
            Start::Command(id) if REFUSED_COMMANDS.contains(&id) => {
                Some(format!("{} is not available on ECAD pages: it edits blocks outside the commands ECAD records", id.to_ascii_uppercase()))
            }
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
                ClipSnapshot { doc: self.ledger.document().clone(), page, map: self.pages.get(&page).map(|v| v.map.clone()).unwrap_or_default() },
            );
            while self.clips.len() > 8 {
                self.clips.pop_first();
            }
            self.events.record(EventKind::ClipboardCaptured, Some(c.command), head, Some(page), format!("{} objects", clip.handles.len()));
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
        // OOPS brings back only the plain drawing the last erase on this page removed (brief 023).
        if c.command == "oops" && !self.oops.allows(c.uid, page, &last, &now) {
            self.restore(pg, c.uid, page);
            self.events.record(EventKind::Refused, Some(c.command), head, Some(page), crate::oops::REFUSED);
            return Verdict::Refuse(crate::oops::REFUSED.into());
        }
        let stash = crate::oops::STASH_COMMANDS.contains(&c.command);
        self.tag_high.catch_up(&self.ledger);
        let t = translate::translate(&mut translate::Ctx {
            tags: &mut self.tag_high,
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
                if stash {
                    self.oops.note_refused(c.uid);
                }
                self.events.record(EventKind::Refused, Some(c.command), head, Some(page), m.clone());
                return Verdict::Refuse(m);
            }
        };
        if stash && !t.ops.is_empty() {
            self.oops.note_accepted(c.uid, page, self.ledger.document(), &view.map, &t.ops);
        }
        if t.ops.is_empty() {
            // Only differences that are not recorded (header variables such as the current layer).
            // A new draw order (TEXTTOFRONT, DRAWORDER) is not recorded either: the page is drawn
            // in the projection's order again (brief 018 put derived texts on every page).
            let reordered = !now.model.iter().map(|e| e.handle).eq(last.model.iter().map(|e| e.handle));
            if let Some(v) = self.pages.get_mut(&page) {
                v.last = Some(now);
            }
            if reordered {
                self.refresh_page(pg, page, Some(c.command), None, false);
            }
            self.events.record(EventKind::NoChange, Some(c.command), head, Some(page), "");
            return Verdict::Accept;
        }
        let label = c.command.to_ascii_uppercase();
        // Nothing is drawn on a generated page (brief 024, design 024 §5.3).
        if let Some(m) = crate::generated::refusal(self.ledger.document(), &t.ops) {
            self.restore(pg, c.uid, page);
            self.events.record(EventKind::Refused, Some(c.command), head, Some(page), m.clone());
            return Verdict::Refuse(m);
        }
        // References are derived for the cables the ops could touch only, once before and once
        // after the commit (brief 014): a move of a component outside any cable derives none.
        let scope = crate::references::cables_touched(self.ledger.document(), &t.ops);
        let devices = crate::device_list::devices_touched(self.ledger.document(), &t.ops);
        let props = crate::props::touched(self.ledger.document(), &t.ops);
        let tree = tree_touched(&t.ops);
        let refs_before = crate::references::Snapshot::take(self.ledger.document(), scope.as_ref());
        let checks = self.checks.as_ref().map(|c| c.scope(self.ledger.document(), &t.ops));
        let outputs = self.outputs.as_ref().map(|o| o.scope(self.ledger.document(), &t.ops));
        let n = t.ops.len();
        // The numbering pass (brief 022) extends the same transaction.
        if let Err(e) = self.ledger.commit_extended(self.author.clone(), &label, t.ops, now_ms(), ecad_model::wiring::assign) {
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
        // A reference end on another page may be drawn from this page's objects (brief 010). Only
        // the active page is drawn (brief 016); the change log says which other pages changed,
        // for a preview of one of them.
        let refs_after = crate::references::Snapshot::take(self.ledger.document(), scope.as_ref());
        self.update_checks(checks);
        self.update_outputs(outputs);
        let changed = touched(refs_before.pages_changed(&refs_after), self.numbered_pages(n)).map(|mut c| {
            c.insert(page);
            c
        });
        let props = self.with_numbered(props, n);
        self.record_change(changed, devices, props, tree);
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

    fn save(&mut self, s: &mut Session, _uid: u64, path: Option<&str>, save_as: bool) -> cadcraft_engine::Result<Value> {
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
        // Closing the page window closes the project (experiment choice, proposal §18.4), which
        // prompts when unsaved; with `keep_open` (the trial, design 016 §4.1) it only hides the
        // page and the project stays open, so nothing is lost and nothing is asked.
        self.unsaved() && self.open_pages() <= 1 && !self.keep_open
    }

    fn closed(&mut self, s: &mut Session, uid: u64) -> bool {
        self.oops.forget(uid);
        if let Some(page) = self.page_of(uid) {
            // The page's view is remembered for when it is shown again (design 016 §4.1).
            if let Some(st) = s.docs.iter().find(|d| d.uid == uid) {
                self.page_views.insert(page, st.views.clone());
            }
            if let Some(v) = self.pages.get_mut(&page) {
                *v = PageView::default();
            }
            self.events.record(EventKind::PageClosed, Some("close"), self.ledger.head(), Some(page), "");
        }
        let done = self.open_pages() == 0 && !self.keep_open;
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
                Some(Form::Symbol) => {
                    doc.representations.get(&rep).and_then(|r| r.items.first()).and_then(|i| view.map.item(*i)).is_some_and(|h| dragged.contains(&h))
                }
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
        self.events.record(EventKind::Recovered, None, self.ledger.head(), None, "owner callback failed; pages rebuilt from the document");
    }

    fn label(&self) -> String {
        self.name.clone()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Project {
    /// One undo, as `u` does (used by `undoissue` after its confirmation).
    pub(crate) fn undo_step(&mut self, pg: &mut Pages<'_>) -> Result<(), String> {
        self.history(pg, true).map(|_| ()).map_err(|e| e.to_string())
    }

    fn history(&mut self, pg: &mut Pages<'_>, undo: bool) -> cadcraft_engine::Result<Option<String>> {
        let command = if undo { "undo" } else { "redo" };
        self.oops.stale();
        if self.pages.values().any(|v| v.unavailable) {
            return Err(EngineError::Other("close and reopen the unavailable page first".into()));
        }
        // The ops the undo or redo will apply, scoped as a commit's are (brief 014).
        let ops = if undo { self.ledger.undo_ops() } else { self.ledger.redo_ops() }.unwrap_or_default();
        let scope = crate::references::cables_touched(self.ledger.document(), &ops);
        let pages = pages_touched(self.ledger.document(), &ops);
        let order_before = self.ledger.document().page_order.clone();
        let drawn = pages_named(&ops);
        let devices = crate::device_list::devices_touched(self.ledger.document(), &ops);
        let props = crate::props::touched(self.ledger.document(), &ops);
        let tree = tree_touched(&ops);
        let refs_before = crate::references::Snapshot::take(self.ledger.document(), scope.as_ref());
        let checks = self.checks.as_ref().map(|c| c.scope(self.ledger.document(), &ops));
        let outputs = self.outputs.as_ref().map(|o| o.scope(self.ledger.document(), &ops));
        // Undoing an issued revision stops and asks first (owner answer Q7).
        if undo
            && let Some((rev, n)) = crate::revisions::issue_in(&crate::revisions::inverse(&ops))
            && !std::mem::take(&mut self.confirm_issue_undo)
        {
            let date = self.ledger.document().properties.get(&crate::revisions::key(n, "date")).cloned();
            let q = crate::sheet_cmds::issue_undo_question(&rev, date.as_deref().unwrap_or_default());
            self.issue_undo_question = Some(q.clone());
            return Err(EngineError::Other(format!("{q} Use Undo Issue (`undoissue`) to confirm.")));
        }
        self.issue_undo_question = None;
        // Undoing a merge stops and asks first (design 020 §4.3).
        if undo
            && !std::mem::take(&mut self.confirm_merge_undo)
            && let Some(q) = crate::merging::undo_question(self)
        {
            self.merge_undo_question = Some(q.clone());
            return Err(EngineError::Other(format!("{} Use Undo Merge (`undomerge`) to confirm.", q.replace('\n', " "))));
        }
        self.merge_undo_question = None;
        self.note_frame_ops(&ops);
        let r = if undo { self.ledger.undo_at(self.author.clone(), now_ms()) } else { self.ledger.redo_at(self.author.clone(), now_ms()) };
        match r {
            Ok(_) => {
                let label = self.ledger.log().last().map(|t| t.label.clone()).unwrap_or_default();
                let kind = if undo { EventKind::Undo } else { EventKind::Redo };
                self.events.record(kind, Some(command), self.ledger.head(), None, label.clone());
                let refs_after = crate::references::Snapshot::take(self.ledger.document(), scope.as_ref());
                self.update_checks(checks);
                self.update_outputs(outputs);
                let was_active = self.pages.iter().find(|(_, v)| v.uid.is_some()).map(|(p, _)| *p);
                let target = front_after_history(&order_before, &self.ledger.document().page_order, Some(&drawn), self.active_page());
                self.record_change(touched(pages, refs_before.pages_changed(&refs_after)), devices, props, tree);
                match target {
                    // The page the undo or redo changed comes to the front, so the change is seen
                    // (brief 016, AC8). A page that cannot be shown leaves the active page as it is.
                    Some(page) if self.open_page(pg, page).is_ok() => {
                        // A page already in the window was only brought to the front: draw it.
                        if was_active == Some(page) {
                            self.redraw_window(pg, Some(command));
                        }
                        let words = crate::references::page_words(self.ledger.document(), page);
                        pg.echo(format!("{} brought to the front.", words.replacen("page", "Page", 1)));
                    }
                    _ => self.redraw_window(pg, Some(command)),
                }
                if let Some(line) = refs_before.orphaned_line(self.ledger.document(), &refs_after) {
                    pg.echo(line);
                }
                Ok(Some(label))
            }
            Err(ecad_model::ModelError::NothingTo(_)) => Ok(None),
            Err(e) => Err(EngineError::Other(e.to_string())),
        }
    }
}

/// The pages whose drawing `ops` could change directly (their items and representations, and the
/// pages a conductor whose ends they attach is routed on), worked out on the document before them;
/// `None` when that could be any page (op kinds other than item, representation, attachment and
/// new-object ops). Reference texts on other pages are [`crate::references::Snapshot`]'s part.
pub(crate) fn pages_touched(doc: &ecad_model::Document, ops: &[Op]) -> Option<BTreeSet<PageId>> {
    let mut out = BTreeSet::new();
    for op in ops {
        match op {
            Op::AddItem { item, .. } | Op::RemoveItem { item, .. } => {
                out.insert(item.page);
            }
            Op::ModifyItem { before, after, .. } => {
                out.insert(before.page);
                out.insert(after.page);
            }
            Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => {
                out.insert(rep.page);
            }
            Op::ModifyRepresentation { before, after, .. } => {
                out.insert(before.page);
                out.insert(after.page);
            }
            Op::AttachEnd { conductor, .. } => {
                out.extend(doc.representations_of(Subject::Conductor(*conductor)).map(|(_, r)| r.page));
            }
            // A continuation's partners list it on their pages (brief 022): any page.
            Op::CreateComponent { component, .. } | Op::DeleteComponent { component, .. }
                if ecad_model::connectivity::continuation_key(&component.properties).is_some() =>
            {
                return None;
            }
            Op::CreateConductor { id, .. } if !doc.conductors.contains_key(id) => {}
            Op::CreateComponent { id, .. } if !doc.components.contains_key(id) => {}
            Op::AddTerminal { id, .. } if !doc.terminals.contains_key(id) => {}
            Op::EmbedDefinition { id, .. } if !doc.definitions.contains_key(id) => {}
            // A page's own properties show in its frame only (brief 019); the references they
            // change on other pages are the reference snapshot's part.
            // A folder move changes the book order (Next boxes, unnumbered pages) anywhere.
            Op::SetProperty { object: ecad_model::ObjectRef::Page(_), key, .. } if key == ecad_model::book::FOLDER_KEY => {
                return None;
            }
            Op::SetProperty { object: ecad_model::ObjectRef::Page(p), key, .. } => {
                out.insert(*p);
                // The page before it in book order shows its number in its Next box.
                if key == ecad_model::book::NUMBER_KEY {
                    let book = ecad_model::book::Book::of(doc);
                    if let Some(prev) = book.position(*p).and_then(|i| i.checked_sub(2)).and_then(|i| book.order.get(i)) {
                        out.insert(*prev);
                    }
                }
            }
            // A device's tag is drawn beside each of its symbols (brief 018): their pages. A
            // continuation's name is drawn beside its partners too (brief 022): any page.
            Op::SetProperty { object: ecad_model::ObjectRef::Component(_), key, .. }
                if key == ecad_model::connectivity::NAME || key == ecad_model::connectivity::KIND =>
            {
                return None;
            }
            Op::SetProperty { object: ecad_model::ObjectRef::Component(c), .. } => {
                out.extend(doc.representations_of(Subject::Component(*c)).map(|(_, r)| r.page));
            }
            // A conductor's label shows its properties (brief 022): its pages.
            Op::SetProperty { object: ecad_model::ObjectRef::Conductor(c), .. } => {
                out.extend(doc.representations_of(Subject::Conductor(*c)).map(|(_, r)| r.page));
            }
            // Brief 026: a cable's tag is drawn by its slashes and its members' labels: their
            // pages (its references elsewhere are the reference snapshot's part). A high-water
            // reset is drawn nowhere.
            Op::SetProperty { object: ecad_model::ObjectRef::Cable(k), .. } => {
                out.extend(doc.representations_of(Subject::Cable(*k)).map(|(_, r)| r.page));
                if let Some(cable) = doc.cables.get(k) {
                    for m in &cable.members {
                        out.extend(doc.representations_of(Subject::Conductor(m.conductor)).map(|(_, r)| r.page));
                    }
                }
            }
            Op::SetProperty { object: ecad_model::ObjectRef::Project, key, .. } if key.starts_with(ecad_model::tag::HIGH_RESET_PREFIX) => {}
            _ => return None,
        }
    }
    Some(out)
}

/// The pages whose drawn objects `ops` add, change or remove (items and representations): where
/// an undo or redo of them shows. Other ops (subjects, attachments, cables, properties) are drawn
/// through those objects or nowhere, so they name no page.
pub(crate) fn pages_named(ops: &[Op]) -> BTreeSet<PageId> {
    let mut out = BTreeSet::new();
    for op in ops {
        match op {
            Op::AddItem { item, .. } | Op::RemoveItem { item, .. } => {
                out.insert(item.page);
            }
            Op::ModifyItem { before, after, .. } => {
                out.insert(before.page);
                out.insert(after.page);
            }
            Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => {
                out.insert(rep.page);
            }
            Op::ModifyRepresentation { before, after, .. } => {
                out.insert(before.page);
                out.insert(after.page);
            }
            _ => {}
        }
    }
    out
}

/// The page an undo or redo brings to the front (design 016 §4.3), or `None` to stay: a page it
/// adds; the page before the active page when it removes the active page (else the first page);
/// otherwise, when it changed pages that do not include the active page, the first of them in
/// page order. Rename and move (a page removed and added again) stay; so does a change that could
/// be on any page (`touched` `None`), and reference texts changing elsewhere never switch.
pub(crate) fn front_after_history(before: &[PageId], after: &[PageId], touched: Option<&BTreeSet<PageId>>, active: Option<PageId>) -> Option<PageId> {
    if let Some(added) = after.iter().find(|p| !before.contains(p)) {
        return Some(*added);
    }
    if let Some(a) = active
        && !after.contains(&a)
    {
        let at = before.iter().position(|p| *p == a).unwrap_or(0);
        let earlier = before.get(..at).unwrap_or_default().iter().rev().find(|p| after.contains(p));
        return earlier.or(after.first()).copied();
    }
    let touched = touched?;
    if active.is_some_and(|a| touched.contains(&a)) {
        return None;
    }
    after.iter().find(|p| touched.contains(p)).copied()
}

/// `pages` and `refs` together; `None` (every page) when either is.
fn touched(pages: Option<BTreeSet<PageId>>, refs: Option<BTreeSet<PageId>>) -> Option<BTreeSet<PageId>> {
    let (mut a, b) = (pages?, refs?);
    a.extend(b);
    Some(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Design 016 section 4.3, table-driven: which page an undo or redo brings to the front.
    #[test]
    fn the_page_an_undo_or_redo_brings_to_the_front() {
        let [a, b, c] = [PageId(1), PageId(2), PageId(3)];
        let set = |p: &[PageId]| p.iter().copied().collect::<BTreeSet<_>>();
        let cases: &[(&str, &[PageId], &[PageId], Option<BTreeSet<PageId>>, Option<PageId>, Option<PageId>)] = &[
            ("edit on the active page stays", &[a, b], &[a, b], Some(set(&[a])), Some(a), None),
            ("edit on another page switches", &[a, b], &[a, b], Some(set(&[b])), Some(a), Some(b)),
            ("first touched in page order", &[a, b, c], &[a, b, c], Some(set(&[c, b])), Some(a), Some(b)),
            ("nothing drawn changed stays", &[a, b], &[a, b], Some(set(&[])), Some(a), None),
            ("unknown pages stay", &[a, b], &[a, b], None, Some(a), None),
            ("a page added becomes active", &[a, b], &[a, b, c], Some(set(&[])), Some(a), Some(c)),
            ("active removed: the page before", &[a, b, c], &[a, b], Some(set(&[])), Some(c), Some(b)),
            ("active first removed: the first", &[a, b], &[b], Some(set(&[])), Some(a), Some(b)),
            ("rename or move stays", &[a, b], &[b, a], Some(set(&[])), Some(a), None),
            ("no window: touched page", &[a, b], &[a, b], Some(set(&[b])), None, Some(b)),
        ];
        for (what, before, after, touched, active, want) in cases {
            assert_eq!(front_after_history(before, after, touched.as_ref(), *active), *want, "{what}");
        }
    }
}
