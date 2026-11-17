//! The ECAD project as a CADCraft document owner.
//!
//! [`Project`] holds the authoritative [`Ledger`] and one view per page. A page that is open in the
//! engine is a CADCraft document whose drawing is a projection of the ECAD document; the
//! [`HandleMap`] relates its handles to ECAD IDs and lives only in the session (proposal §13).

use crate::draw::text_style::LookApply as _;
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, Weak};

use cadcraft_engine::doc::{Block, Common, Drawing, Entity, EntityKind, EntityStore, Group, Handle, Layer as CLayer, Lineweight, Text};
use cadcraft_engine::ext::{Commit, DocumentOwner, OwnerRef, Start, Verdict};
use cadcraft_engine::{EngineError, Session};

use crate::access::{Pages, PagesRef, ProjectHandle};
use ecad_electrical::CmdError;
use ecad_inspect::{EventKind, EventLog, LiveFacts, PageInfo};
use ecad_model::{
    Author, CableId, DeviceId, Document, End, Form, IdAllocator, ItemKind, Layer, LayerId, Ledger, LengthUnit, Op, Page, PageId, ProjectId,
    ProjectSettings, Pt, RepresentationId, Subject, TerminalId, WireId,
};
use ecad_store::MemoryBackend;
use serde_json::{Value, json};

use crate::convert::{ECAD_PREFIX, MARKER_LAYER, block_name, entity_for, kind_for_primitive, marker_entity};
use crate::translate;
pub use crate::views::View;

mod changes;
mod handle_map;
mod open;
mod owner;
mod page_tree;
mod persistence;

pub use changes::Change;
use changes::{Old, Path, pass_through};
pub use handle_map::{HandleMap, Mapped};
pub use owner::{LISP_REFUSED, REFUSED_COMMANDS};
pub use page_tree::{PageRow, PageView, TreeRow, tree_rows, tree_touched};
pub use persistence::{ClipSnapshot, Faults, MemoryStore, Persistence, SAVE_UNAVAILABLE};

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
    /// The test-only project file in memory (`Persistence::TestStore`).
    pub store: MemoryStore,
    /// The project's file on disk (`Persistence::File`).
    pub file: crate::file::Binding,
    /// What opening reported, when the project was opened from a file on disk or from the test
    /// store.
    pub open_report: Option<ecad_file::ReportV2>,
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
    /// Show Hidden Text (brief 031, design §5.7): pages also draw their hidden derived texts,
    /// dimmed. A session display state like `views`: never in the ledger or the file, not
    /// undoable, never printed. Off when a project is opened.
    pub show_hidden_text: bool,
    /// Where this user's cable views are remembered ([`crate::views::Settings`]).
    pub settings: crate::views::Settings,
    /// Whether [`crate::views::NOT_REMEMBERED`] was said in this session.
    pub(crate) said_not_remembered: bool,
    /// Who the project's edits, undos and redos are recorded as (FILE-07): [`Author::User`] by
    /// default, `Author::Agent(<client name>)` when an agent drives the session (brief 011). A
    /// session setting, not part of the design: set it through [`ProjectHandle::configure`].
    pub author: Author,
    /// Closing the page window leaves the project open with no active page (design 016 §4.1)
    /// instead of closing the project. Off by default (tests, MCP); the desktop app turns it on and off
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
    /// Brief 052 (design 052 §5.9): the use index of the project's list of functions and
    /// locations, read from the log as it grows; rebuilt on a branch switch.
    pub(crate) designation_uses: crate::designations::Uses,
    /// Brief 052d (design 052 §5.5): the devices whose stored value differs from what the last
    /// placement, move or box proposed, waiting for Move or Leave (Leave by default; any other
    /// edit leaves them).
    pub conflicts: Vec<crate::proposals::Conflict>,
    /// Bumped whenever a page row could differ ([`Project::page_list_version`]).
    page_list_version: u64,
    /// What OOPS may bring back on each page document (brief 023, [`crate::oops`]).
    oops: crate::oops::Records,
    /// What the last placement in this session placed (symbol key, device), for the prompts'
    /// defaults (design 018 §5.3).
    pub(crate) last_placed: Option<(String, DeviceId)>,
    /// The last continuation name placed in this session, the prompt's default (design 022 §3.2).
    pub(crate) last_continuation: Option<String>,
    /// Brief 049: the last caption given in this session, Caption Continuations' default.
    pub(crate) last_caption: Option<String>,
    /// The Wire tool's potential for new wires in this session; `None` is `auto` (design 022 §6).
    pub wire_potential: Option<String>,
    /// The Checks list (design 022 §8), built on its first show and then kept from each change's
    /// ops ([`crate::checks::Checks::update`]); `None` until first shown.
    pub checks: Option<crate::checks::Checks>,
    /// The report models (brief 024, [`crate::outputs`]): built on first show (Reports, a BOM
    /// page, Checks) and then kept from each change's ops; `None` until then.
    pub outputs: Option<crate::outputs::Outputs>,
    /// `reports` asked for the Reports window (the desktop app opens it and clears this).
    pub open_reports: bool,
    /// Brief 035: `agentaccess` asked for this agent access preference (the desktop app applies it and
    /// clears this).
    #[cfg(not(target_arch = "wasm32"))]
    pub agent_access_request: Option<crate::agent_access::Preference>,
    /// Brief 035: the desktop app's agent access state as `agentaccess` reports it (set by the desktop app).
    #[cfg(not(target_arch = "wasm32"))]
    pub agent_access_status: Option<crate::agent_access::Status>,
    /// The Checks tab's go-to ring (design 022 §8): page, point and the row's severity, until the
    /// next change. Display only, never recorded.
    pub goto_ring: Option<(PageId, Pt, crate::checks::Severity)>,
    /// The active page's frame as last drawn (brief 019): page, what it showed, its parts. Kept
    /// until an op of the frame's update map ([`crate::frame::touches`]) or another page.
    frame: Option<(PageId, crate::frame::FrameInput, Arc<Vec<crate::frame::Part>>)>,
    /// An op of the update map was committed since the frame was drawn.
    frame_stale: bool,
    /// Brief 054b: the wire categories in use, kept while the project has a cover.
    cover_uses: Option<crate::front::Uses>,
    /// Brief 033 (design 033 §6): the drawing changed since the newest issued revision, as the
    /// title block shows it; worked out on first use and after every change.
    pub pending: bool,
    pending_known: bool,
    /// How many times a frame was worked out again (tests and bench: only when it can differ).
    pub frame_builds: u64,
    /// How many times a page in the window was projected again after a change or a view switch
    /// ([`Project::refresh_page`]; tests: a commit that touched another page projects nothing).
    pub projections: u64,
    /// The revision opened read-only (design 019 §7.3): session only, never recorded.
    pub revision_view: Option<crate::cmd::sheet::RevisionView>,
    /// How many transactions the command now running may record, when its caller caps them (an
    /// agent's call through MCP); `None` for no cap. Commands that write files check it before
    /// they write (brief 027's release), so nothing reaches the disk that the cap would take back.
    pub tx_budget: Option<usize>,
    /// Plain `u` across an issued revision stops and asks (owner answer Q7): set by the desktop app's
    /// confirmation, used by the next undo only.
    pub confirm_issue_undo: bool,
    /// The question an undo across an issue is waiting on, for the desktop app's popup.
    pub issue_undo_question: Option<String>,
    /// Plain `u` across a merge stops and asks (design 020 §4.3): set by `undomerge`, used by
    /// the next undo only.
    pub confirm_merge_undo: bool,
    /// The question an undo across a merge is waiting on, for the desktop app's popup.
    pub merge_undo_question: Option<String>,
    /// The merge last prepared (its states and scope), kept while nothing it read changes
    /// (brief 020b).
    pub merge_cache: Option<Arc<crate::merging::Prepared>>,
    /// The general reference index (brief 025): built on open and branch switch, kept per commit
    /// from the ops ([`crate::xref::Watch`]).
    pub xref: crate::xref::RefIndex,
    /// Brief 044 (design 044 §11.1): the connector index, built on open and branch switch, kept
    /// per commit from the ops ([`crate::connectors::Index`]).
    pub connectors: crate::connectors::Index,
    /// What the last commit's [`crate::xref::Watch`] derived after it, with the state it was
    /// derived for (head and last transaction ID): the next commit's "before" while unchanged.
    xref_shown: Option<((u64, u128), crate::xref::Shown)>,
    /// Counts branch actions (create, rename, switch, archive, reopen), however they were run:
    /// what shows the branch (title, bar, Revisions, History) refreshes when it moves (brief 020a).
    branch_gen: u64,
    /// Every derived view, updated by the one commit pipeline (design 064 §5).
    derived: ecad_electrical::derived::Views,
}

/// The derived views of `doc`, built with this engine's inputs (design 064 §5.2).
pub(crate) fn views_for(doc: &Document) -> ecad_electrical::derived::Views {
    let inputs = ecad_electrical::derived::EngineInputs { metrics: Arc::new(crate::draw::text_style::EngineMetrics) };
    let mut views = ecad_electrical::derived::Views::new(inputs);
    views.rebuild(doc);
    views
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
/// lineweight, bends, tails and frames the wire lineweight, text the layer's. A text takes its
/// height and style from `look` (brief 031): a reference end's text is a reference, every other
/// one cable text.
pub(crate) fn part_entity(look: &crate::text_style::Look, part: &crate::look::Part, handle: Handle) -> Entity {
    use crate::look::{Role, Shape, Weight};
    use crate::text_style::TextKind;
    let text_kind = if part.role == Role::ReferenceText { TextKind::Reference } else { TextKind::CableText };
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
        Shape::Text { at, rotation, value } => kind_for_primitive(&Primitive::Text {
            at: *at,
            height: look.style(text_kind).height,
            rotation: *rotation,
            value: value.clone(),
            halign: Default::default(),
            valign: Default::default(),
        }),
    };
    let lineweight = match part.weight {
        Weight::Cable => Lineweight::Mm100(crate::look::CABLE_WEIGHT),
        Weight::Wire => Lineweight::Mm100(crate::look::WIRE_WEIGHT),
        Weight::Text => Lineweight::ByLayer,
    };
    let color = part.color.map_or(cadcraft_engine::doc::color::Color::ByLayer, cadcraft_engine::doc::color::Color::Index);
    let mut e = Entity { handle, common: Common { layer: crate::views::LAYER.into(), lineweight, color, ..Common::default() }, kind };
    if matches!(part.shape, Shape::Text { .. }) {
        look.apply(&mut e, text_kind);
    }
    e
}

/// One part of a device cross-reference (brief 025) as an entity on the tags layer: a text, or a
/// line (the table's bar at the wire lineweight).
/// Brief 066d: a text part is drawn in its kind's text style (font, weight, colour), as every
/// other derived text is.
pub(crate) fn xref_entity(look: &crate::text_style::Look, part: &crate::xref::Part, handle: Handle) -> Entity {
    let mut e = match &part.shape {
        crate::xref::Shape::Text { kind, at, value, height, halign, valign, rotation } => {
            let mut e = crate::devices::text_entity_with(
                value,
                *at,
                *height,
                crate::draw::xref::engine_halign(*halign),
                crate::draw::xref::engine_valign(*valign),
                handle,
                crate::devices::TAG_LAYER,
            );
            // Brief 049 (FP3): a continuation's reference reads along its wire; every other part
            // keeps 0, so its entity is unchanged.
            if let EntityKind::Text(t) = &mut e.kind {
                t.rotation = *rotation;
            }
            look.apply(&mut e, *kind);
            e
        }
        crate::xref::Shape::Line(a, b) => Entity {
            handle,
            common: Common { layer: crate::devices::TAG_LAYER.into(), ..Common::default() },
            kind: kind_for_primitive(&ecad_model::Primitive::Line { a: *a, b: *b }),
        },
    };
    if part.bar {
        e.common.lineweight = Lineweight::Mm100(crate::look::WIRE_WEIGHT);
    }
    if let Some(aci) = part.aci {
        e.common.color = cadcraft_engine::doc::color::Color::Index(aci);
    }
    e
}

/// One link line segment (brief 030) as a dashed hairline on the links layer.
pub(crate) fn link_entity(part: &crate::link_lines::LinkPart, handle: Handle) -> Entity {
    Entity {
        handle,
        common: Common {
            layer: crate::link_lines::LAYER.into(),
            linetype: crate::locations::LINETYPE.into(),
            lineweight: Lineweight::Mm100(crate::look::WIRE_WEIGHT),
            ..Common::default()
        },
        kind: kind_for_primitive(&ecad_model::Primitive::Line { a: part.a, b: part.b }),
    }
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
    // Brief 066d (overseer ruling 3): the built-in `Free` text style; the page could not be read.
    let free = crate::text_style::text_style(&ecad_model::Document::default(), crate::text_style::TextKind::Free);
    let text = Text {
        insert: cadcraft_engine::geom::Vec3::new(0.0, 0.0, 0.0),
        align_pt: None,
        height: free.height,
        value: PLACEHOLDER_TEXT.into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: free.style_name.into(),
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
    project_page_shown(doc, page, base, prev, fan, tol, frame, false)
}

/// [`project_page_with`] with Show Hidden Text (brief 031, design §5.7): `hidden_text` also draws
/// every hidden derived text, dimmed, on [`crate::text_style::HIDDEN_LAYER`] (not plotted). A
/// session display state: print and PDF project with it off.
///
/// # Errors
/// As [`project_page`].
pub fn project_page_with_hidden(
    doc: &Document, page: PageId, base: &Drawing, prev: &HandleMap, fan: &BTreeSet<CableId>, tol: f64, hidden_text: bool,
) -> Result<(Drawing, HandleMap), String> {
    project_page_pending(doc, page, base, prev, fan, tol, hidden_text, false)
}

/// [`project_page_with_hidden`] with the project's pending flag (brief 033, design 033 §6): the
/// frame's Rev box and the cover show the pending marker as the project draws them.
///
/// # Errors
/// As [`project_page`].
#[allow(clippy::too_many_arguments)]
pub fn project_page_pending(
    doc: &Document, page: PageId, base: &Drawing, prev: &HandleMap, fan: &BTreeSet<CableId>, tol: f64, hidden_text: bool, pending: bool,
) -> Result<(Drawing, HandleMap), String> {
    project_page_shown(doc, page, base, prev, fan, tol, &crate::generated::parts_of_pending(doc, page, None, pending), hidden_text)
}

/// [`project_page_framed`] with Show Hidden Text (see [`project_page_with_hidden`]).
///
/// # Errors
/// As [`project_page`].
#[allow(clippy::too_many_arguments)]
pub fn project_page_shown(
    doc: &Document, page: PageId, base: &Drawing, prev: &HandleMap, fan: &BTreeSet<CableId>, tol: f64, frame: &[crate::frame::Part],
    hidden_text: bool,
) -> Result<(Drawing, HandleMap), String> {
    project_page_kept(doc, page, base, prev, fan, tol, frame, hidden_text, None)
}

/// [`project_page_shown`] with the kept PLC model, when built (brief 045).
///
/// # Errors
/// As [`project_page`].
#[allow(clippy::too_many_arguments)]
pub fn project_page_kept(
    doc: &Document, page: PageId, base: &Drawing, prev: &HandleMap, fan: &BTreeSet<CableId>, tol: f64, frame: &[crate::frame::Part],
    hidden_text: bool, plc: Option<&crate::plc::kept::Plc>,
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
    if d.layer(crate::link_lines::LAYER).is_none() {
        d.layers.push(CLayer::new(crate::link_lines::LAYER));
    }
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
        .items_on(page)
        .filter_map(|(_, i)| match &i.kind {
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
    for (id, item) in doc.items_on(page).filter(|(id, _)| !hidden.contains(id)) {
        let h = prev.item(id).unwrap_or_else(|| d.new_handle());
        let e = entity_for(doc, item, h).ok_or_else(|| format!("item {id} cannot be drawn"))?;
        ents.push(e);
        map.insert_item(id, h);
    }
    // Brief 040: wires and cores in their colours, two-colour ones with dashes (wires.rs).
    crate::draw::wires::colour_routes(doc, page, &mut d, &mut ents, &mut map, prev);
    // Brief 050b: wire routes in their category's line type and weight.
    crate::draw::wires::style_routes(doc, page, &mut ents, |i| map.item(i));
    // Brief 050a: location box outlines in their location style.
    crate::locations::style_boxes(doc, page, &mut ents, |i| map.item(i));
    let mut groups = Vec::new();
    for (rid, r) in doc.representations_on(page).filter(|(_, r)| r.form == Form::Drawn) {
        let mut members: Vec<Handle> = r.items.iter().filter_map(|i| map.item(*i)).collect();
        for (t, at) in &r.connection_points {
            let h = prev.marker(rid, *t).unwrap_or_else(|| d.new_handle());
            ents.push(marker_entity(*at, h));
            map.insert_marker(rid, *t, h);
            members.push(h);
        }
        if matches!(r.subject, Subject::Device(_)) && members.len() > 1 {
            groups.push(Group { name: format!("{ECAD_PREFIX}{:032x}", rid.0), description: String::new(), selectable: true, members });
        }
    }
    // The project's text styles (brief 031), for every derived text of the page.
    let look = crate::text_style::Look::new(doc);
    for (k, f) in &fans {
        map.quiet.extend(f.quiet.iter().copied());
        for (i, part) in f.parts.iter().enumerate() {
            // A reference end's text (brief 009's look, brief 010's live text) is mapped to its
            // reference, so hover, Go to Reference and erasing it act on the reference.
            if part.role == crate::look::Role::ReferenceText
                && let Some(r) = part.reference
            {
                let h = prev.ref_text(r).unwrap_or_else(|| d.new_handle());
                ents.push(part_entity(&look, part, h));
                map.insert_ref_text(r, h);
                continue;
            }
            let index = u32::try_from(i).map_err(|_| "a cable drawing has too many parts")?;
            let h = prev.fan(*k, index).unwrap_or_else(|| d.new_handle());
            ents.push(part_entity(&look, part, h));
            map.insert_part(*k, index, h, f.is_fan(), part.core, part.reference);
        }
    }
    // Device tags and terminal labels (brief 018): derived on every draw, never stored, in the
    // project's text styles (brief 031).
    crate::text_style::install_look(&mut d, &look);
    if hidden_text {
        crate::text_style::add_hidden_layer(&mut d);
    }
    for t in crate::devices::all_texts_on_page_with(doc, page, plc) {
        if !t.shown && !hidden_text {
            continue;
        }
        // Brief 050a: an object's own text style in a font of its own.
        if let Some(f) = t.text.own.as_ref().and_then(|o| o.family.as_deref()) {
            crate::text_style::install_family(&mut d, doc, f);
        }
        let h = prev.tag_text(t.text.rep, t.text.of).unwrap_or_else(|| d.new_handle());
        let mut e = crate::devices::text_entity(&look, &t.text, h);
        if !t.shown {
            crate::text_style::dim_hidden(&mut e);
        }
        ents.push(e);
        map.insert_tag_text(t.text.rep, t.text.of, h);
    }
    // Device cross-references (brief 025): texts under contacts and tables under heads, derived
    // on every draw of the page, never stored.
    for part in crate::xref::parts_on_page(doc, page) {
        let h = prev.xref(part.rep, part.index).unwrap_or_else(|| d.new_handle());
        let mut e = xref_entity(&look, &part, h);
        // Brief 049 (review of #85, 3): a continuation's other-end text in the sheet's Ink 2.
        crate::continuations::colour(doc, &part, &mut e);
        ents.push(e);
        map.insert_xref(part.rep, part.index, h);
    }
    // PLC card frames (brief 051): derived on every draw, never stored; mapped like 025's parts.
    for part in crate::plc::look::parts_on_page(doc, page) {
        let h = prev.xref(part.rep, part.index).unwrap_or_else(|| d.new_handle());
        ents.push(xref_entity(&look, &part, h));
        map.insert_xref(part.rep, part.index, h);
    }
    // Link lines (brief 030): derived on every draw, never stored.
    let mut seen: BTreeMap<(DeviceId, u32), u32> = BTreeMap::new();
    for part in crate::link_lines::parts_on_page(doc, page) {
        let i = seen.entry((part.strip, part.n)).or_default();
        let h = prev.link(part.strip, part.n, *i).unwrap_or_else(|| d.new_handle());
        ents.push(link_entity(&part, h));
        map.insert_link(part.strip, part.n, *i, h);
        *i += 1;
    }
    // Location box labels (brief 024): derived on every draw, never stored.
    for (rep, value, at) in crate::locations::labels_on_page(doc, page) {
        let h = prev.tag_text(rep, crate::devices::TextOf::Tag).unwrap_or_else(|| d.new_handle());
        if let Some(f) = crate::locations::label_family(doc, rep) {
            crate::text_style::install_family(&mut d, doc, &f);
        }
        ents.push(crate::locations::styled_label_entity(doc, &look, rep, &value, at, h));
        map.insert_tag_text(rep, crate::devices::TextOf::Tag, h);
    }
    // Looks-connected marks (brief 032 §2.6): derived with the page, after its quiet ends.
    map.looks = crate::wires::looks_connected_on_page(doc, page, &map.quiet);
    // Wire labels (brief 022): derived on every draw of the page, never stored.
    // A cable in fan view hides its members' routes, and their labels with them (owner,
    // 2026-11-09): display only, nothing is written.
    for l in crate::wires::labels_on_page_with(doc, page, &hidden) {
        if let Some(f) = l.own.as_ref().and_then(|o| o.family.as_deref()) {
            crate::text_style::install_family(&mut d, doc, f);
        }
        let h = prev.label(l.rep).unwrap_or_else(|| d.new_handle());
        ents.push(crate::wires::label_entity(&look, &l, h));
        map.insert_label(l.rep, l.conductor, h);
    }
    ents.sort_by_key(|e| e.handle);
    let mut store = EntityStore::new();
    // The sheet frame and title block (brief 019): derived, never stored, and drawn first so
    // everything else lies on top of it (picks find the drawing, not the frame).
    // Brief 050a: the frame style's line colour, on the frame's own lines (ink otherwise).
    let frame_rgb = crate::frame::line_colour(doc, page);
    for (i, part) in frame.iter().enumerate() {
        let index = u32::try_from(i).map_err(|_| "the frame has too many parts")?;
        let h = prev.frame_part(index).unwrap_or_else(|| d.new_handle());
        store.push(crate::frame::styled_entity(part, h, frame_rgb));
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
    /// Project `page` with this project's cable views and its frame (worked out again only when
    /// the frame's update map says it may differ, or for another page).
    fn project(&mut self, page: PageId, base: &Drawing, prev: &HandleMap) -> Result<(Drawing, HandleMap), String> {
        let parts = self.frame_parts(page);
        // Brief 045 (review of #84, 3): a page with PLC channel cells reads its addresses from the
        // kept PLC model, built on its first show.
        if self.outputs.is_none() && crate::plc::page_has_cells(self.ledger.document(), page) {
            self.outputs();
        }
        let plc = self.outputs.as_ref().map(|o| &o.plc);
        project_page_kept(self.ledger.document(), page, base, prev, &self.views, self.tolerance, &parts, self.show_hidden_text, plc)
    }

    /// The frame parts of `page`, from the kept copy when nothing it shows can have changed.
    fn frame_parts(&mut self, page: PageId) -> Arc<Vec<crate::frame::Part>> {
        if let Some((p, _, parts)) = &self.frame
            && *p == page
            && !self.frame_stale
        {
            return parts.clone();
        }
        // A BOM page's sheet count (and so every page's Total) needs the report models (brief 024);
        // once built they track their pages themselves.
        if self.outputs.is_none() && crate::generated::has_bom_page(self.ledger.document()) {
            self.outputs();
        }
        if !self.pending_known {
            self.refresh_pending();
        }
        let doc = self.ledger.document();
        let (input, total) = crate::generated::input_of(doc, page, self.outputs.as_ref());
        let input = input.with_pending(doc, self.pending);
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

    /// Turn Show Hidden Text on or off (brief 031, design §5.7): display only, like a cable view:
    /// no transaction, no head move, no unsaved change. The open page is redrawn now, the others
    /// when they become active. Returns whether it changed.
    pub(crate) fn set_hidden_text(&mut self, pg: &mut Pages<'_>, on: bool) -> bool {
        if self.show_hidden_text == on {
            return false;
        }
        self.show_hidden_text = on;
        self.record_change(None, crate::device_list::DeviceTouch::default(), Some(BTreeSet::new()), false);
        self.redraw_window(pg, Some("hiddentext"));
        true
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

    /// Bring the tag numbering up to date with the log.
    fn catch_up_tags(&mut self) {
        self.tag_high.catch_up(&self.ledger);
    }

    /// Record one change and the pages it could change.
    ///
    /// The tag numbering ([`crate::devices::TagHigh`]) reads the transactions this change added,
    /// so reads (prompts, previews, pick lists) never read the log. The Pages tree's version moves
    /// only with a change of its update map (`tree`, brief 019, or pages added or removed).
    /// Work out the pending flag again (design 033 §6.3): O(1) on an ordinary edit, the walk
    /// stops at the first decisive transaction. A flip redraws the frame.
    pub(crate) fn refresh_pending(&mut self) {
        let now = crate::revisions::is_pending(&self.ledger);
        if self.pending_known && now != self.pending {
            self.frame_stale = true;
        }
        self.pending = now;
        self.pending_known = true;
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

    fn take_projection_fault(&mut self) -> bool {
        if self.faults.fail_projections > 0 {
            self.faults.fail_projections -= 1;
            true
        } else {
            false
        }
    }

    /// The generated pages whose content the last transaction changed, from the generators' own
    /// update maps: `outputs`, the pages drawn from the report models
    /// ([`Project::update_outputs`]), and the cover, contents and revision pages
    /// ([`crate::generated::touches`] on the transaction's ops). The page list is read only when
    /// the ops can change such a page.
    fn generated_touched(&mut self, outputs: BTreeSet<PageId>) -> BTreeSet<PageId> {
        use crate::generated::GenKind;
        let mut out = outputs;
        let doc = self.ledger.document();
        let ops = self.ledger.log().last().map(|t| t.ops.as_slice()).unwrap_or_default();
        let mut kinds: Vec<GenKind> = [GenKind::Contents, GenKind::Revisions].into_iter().filter(|k| crate::generated::touches(ops, *k)).collect();
        // Brief 054b (design 054 §10.2): the cover's map reads the document after the commit
        // (a definition's or the cables' count crossing zero).
        let has_cover = doc.page_order.iter().any(|p| crate::generated::kind_of(doc, *p) == Some(GenKind::Cover));
        // Brief 054b (design 054 §10.1): the wire categories in use, kept while there is a cover;
        // the legend redraws only when a category's count crosses zero.
        let wires = match (&mut self.cover_uses, has_cover) {
            (Some(u), true) => u.update(doc, ops),
            (None, true) => {
                self.cover_uses = Some(crate::front::Uses::build(doc));
                true
            }
            (_, false) => {
                self.cover_uses = None;
                false
            }
        };
        let doc = self.ledger.document();
        let cover = has_cover && (wires || crate::front::touches(doc, ops));
        if cover {
            kinds.push(GenKind::Cover);
        }
        if !kinds.is_empty() {
            out.extend(doc.page_order.iter().copied().filter(|p| crate::generated::kind_of(doc, *p).is_some_and(|k| kinds.contains(&k))));
        }
        if cover
            && let Some((p, _, _)) = &self.frame
            && crate::generated::kind_of(doc, *p) == Some(GenKind::Cover)
        {
            self.frame_stale = true;
        }
        // Brief 054b: the cover's Checks rows follow the cover (a cover added or removed too).
        let cover_page = |op: &Op| match op {
            Op::AddPage { page, .. } | Op::RemovePage { page, .. } => page.properties.get(crate::generated::KEY).map(String::as_str) == Some("cover"),
            _ => false,
        };
        if cover || ops.iter().any(cover_page) {
            let doc = self.ledger.document();
            if let Some(c) = self.checks.as_mut() {
                c.set_cover(doc);
            }
        }
        out
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
    /// sheet count marks the frame (its Total) stale. Returns the generated pages drawn from the
    /// models whose content changed (overseer item O1: they join the commit's touched pages).
    fn update_outputs(&mut self, scope: Option<crate::outputs::Scope>) -> BTreeSet<PageId> {
        let (Some(scope), Some(mut o)) = (scope, self.outputs.take()) else { return BTreeSet::new() };
        let ledger = &self.ledger;
        let ops = ledger.log().last().map(|t| t.ops.as_slice()).unwrap_or_default();
        // Brief 028: the terminal plan and cable schedule pages count their sheets from the strip
        // and cable rows, so the book's Total is compared whenever such a page exists.
        // Only the pages drawn from the models can change their sheet count here (review of PR
        // #55, finding 1): the kept set, each counted from kept totals; none, no work.
        let doc = ledger.document();
        let sheets = |o: &crate::outputs::Outputs| -> usize {
            o.generated_pages().iter().filter(|p| doc.pages.contains_key(p)).map(|p| crate::generated::sheets(doc, *p, Some(o))).sum()
        };
        let (before, revision) = (sheets(&o), o.revision);
        o.update(doc, ops, scope);
        // Brief 051 (review of #89, 3): the rack sheet prints each card's place.
        let plc_places_moved = crate::plc::sheets::rack_places_touched(doc, &o.plc, ops);
        if ecad_model::ledger::verify_mode()
            && let Some(line) = o.verify_strips(doc)
        {
            self.events.record(EventKind::IndexRebuilt, None, ledger.head(), None, line);
        }
        // The shown generated page redraws only when what it shows changed (tester 028, defect
        // 2): the BOM page on any row change (024), a terminal plan when a strip it shows changed,
        // a cable schedule when a cable row changed.
        // Every such page says whether it was rewritten (overseer item O1), from the kept set.
        let rewritten = |p: PageId| match crate::generated::kind_of(doc, p) {
            Some(crate::generated::GenKind::BomLocation) => o.revision != revision,
            // Brief 046 (design 046 §9): either style, by its own kept change set.
            Some(crate::generated::GenKind::Terminals) => crate::generated::terminals_rewritten(doc, p, &o),
            Some(crate::generated::GenKind::Cables) => o.last_cables_changed,
            // Brief 045c: the I/O list when a card's rows changed.
            Some(crate::generated::GenKind::IoList) => !o.plc.rows_changed.is_empty(),
            // Brief 051 (design 051 §9.3): the rack sheet when a card or rack changed.
            Some(crate::generated::GenKind::PlcRack) => !o.plc.last_changed.is_empty() || !o.plc.rows_changed.is_empty() || plc_places_moved,
            _ => false,
        };
        let changed: BTreeSet<PageId> = o.generated_pages().iter().copied().filter(|p| doc.pages.contains_key(p) && rewritten(*p)).collect();
        let shown_changed = self.frame.as_ref().is_some_and(|(p, _, _)| rewritten(*p));
        if sheets(&o) != before || shown_changed || o.numbers_moved {
            self.frame_stale = true;
        }
        self.outputs = Some(o);
        changed
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
                        project_page_pending(doc, *p, &st.doc, &v.map, &self.views, self.tolerance, self.show_hidden_text, self.pending)
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
        for (cid, c) in &doc.wires {
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

/// Whether definition `id` is used by nothing once `ops` are applied to `doc`: every item and
/// component using it now is removed or moved off it by `ops` (brief 048, FP8's removal of a
/// superseded version in the update's own transaction).
fn unused_after(doc: &ecad_model::Document, id: ecad_model::DefinitionId, ops: &[Op]) -> bool {
    let items = doc.items_using(id);
    let comps = doc.components_using(id);
    let item_left = |i: &ecad_model::ItemId| {
        !ops.iter().any(|o| match o {
            Op::RemoveItem { id: x, .. } => x == i,
            Op::ModifyItem { id: x, after, .. } => x == i && !matches!(&after.kind, ecad_model::ItemKind::Symbol(sym) if sym.definition == id),
            _ => false,
        })
    };
    let comp_left = |c: &ecad_model::DeviceId| {
        !ops.iter().any(|o| match o {
            Op::DeleteDevice { id: x, .. } => x == c,
            Op::SetDeviceDefinition { id: x, after, .. } => x == c && *after != Some(id),
            _ => false,
        })
    };
    !items.iter().any(item_left) && !comps.iter().any(comp_left)
}

/// The pages whose drawing `ops` could change directly (their items and representations, and the
/// pages a conductor whose ends they attach is routed on), worked out on the document before them;
/// `None` when that could be any page (op kinds other than item, representation, attachment and
/// new-object ops). Reference texts on other pages are [`crate::references::Snapshot`]'s part.
/// Brief 045: `plc`, the kept PLC model when built, gives a rack's cards without a walk.
pub(crate) fn pages_touched(doc: &ecad_model::Document, ops: &[Op], plc: Option<&crate::plc::kept::Plc>) -> Option<BTreeSet<PageId>> {
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
                out.extend(doc.representations_of(Subject::Wire(*conductor)).map(|(_, r)| r.page));
            }
            // Brief 049 (design 049 §8.2): a continuation's partners list it on their pages; those
            // pages come from the reference index's watch (`xref::Watch`), and its own pages from
            // its representation ops.
            Op::CreateDevice { device: component, .. } | Op::DeleteDevice { device: component, .. }
                if ecad_model::connectivity::continuation_key(&component.properties).is_some() => {}
            Op::CreateWire { id, .. } if !doc.wires.contains_key(id) => {}
            // Brief 032: a deleted conductor's routes are unlinked and its ends detached in the
            // same transaction (the invariant requires it), and those ops name its pages.
            Op::DeleteWire { .. } => {}
            Op::CreateDevice { id, .. } if !doc.devices.contains_key(id) => {}
            Op::AddTerminal { id, .. } if !doc.terminals.contains_key(id) => {}
            // Brief 028: a terminal relabelled (Remove + Add of one ID) is drawn by its device's
            // symbols only: their pages.
            Op::AddTerminal { terminal, .. } | Op::RemoveTerminal { terminal, .. } => {
                out.extend(doc.representations_of(Subject::Device(terminal.component)).map(|(_, r)| r.page));
            }
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
            // A device's tag is drawn beside each of its symbols (brief 018): their pages. Brief
            // 049 (design 049 §8.2): a continuation's name and kind are drawn on its own pages; its
            // partners' pages come from the reference index's watch.
            Op::SetProperty { object: ecad_model::ObjectRef::Device(c), key, .. }
                if (key == ecad_model::connectivity::NAME || key == ecad_model::connectivity::KIND)
                    && crate::continuations::is_continuation(doc, *c) =>
            {
                out.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page));
            }
            Op::SetProperty { object: ecad_model::ObjectRef::Device(_), key, .. }
                if key == ecad_model::connectivity::NAME || key == ecad_model::connectivity::KIND =>
            {
                return None;
            }
            Op::SetProperty { object: ecad_model::ObjectRef::Device(c), key, after, .. } => {
                out.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page));
                // Brief 045 (design 045 §11.1): a rack's tag shows in its cards' captions, and a
                // card's slot or size moves the addresses of the cards after it.
                out.extend(crate::plc::pages_touched(doc, plc, *c, key, after.as_deref()));
            }
            Op::SetDeviceDefinition { id: c, .. } => {
                out.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page));
            }
            // Design 072: a device's typed fields (its tag, its type) show beside its symbols; a
            // rack's tag also in its cards' captions.
            Op::ModifyDevice { id: c, before, after } => {
                out.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page));
                if before.tag != after.tag {
                    out.extend(crate::plc::pages_touched(doc, plc, *c, ecad_model::tag::TAG_KEY, after.tag()));
                }
            }
            // Brief 048 (FP8): a superseded version nothing shows after the transaction is drawn
            // nowhere. Its last copies move off it in the same transaction (review of PR #86,
            // finding 1), and their own ops name their pages; the document here is the one before.
            Op::RemoveDefinition { id, .. } if unused_after(doc, *id, ops) => {}
            // A conductor's label shows its properties (brief 022): its pages.
            Op::SetProperty { object: ecad_model::ObjectRef::Wire(c), .. } | Op::ModifyWire { id: c, .. } => {
                out.extend(doc.representations_of(Subject::Wire(*c)).map(|(_, r)| r.page));
            }
            // Brief 026: a cable's tag is drawn by its slashes and its members' labels: their
            // pages (its references elsewhere are the reference snapshot's part). A high-water
            // reset is drawn nowhere.
            Op::SetProperty { object: ecad_model::ObjectRef::Cable(k), .. } => {
                out.extend(doc.representations_of(Subject::Cable(*k)).map(|(_, r)| r.page));
                if let Some(cable) = doc.cables.get(k) {
                    for m in &cable.members {
                        out.extend(doc.representations_of(Subject::Wire(m.conductor)).map(|(_, r)| r.page));
                    }
                }
            }
            Op::SetProperty { object: ecad_model::ObjectRef::Project, key, .. } if key.starts_with(ecad_model::tag::HIGH_RESET_PREFIX) => {}
            // Brief 052 (design 052 §8.2a): a rename in the project's list redraws the pages of
            // the devices, boxes and pages storing that entry (every page when it is the project's
            // default); a description is drawn on no page.
            Op::SetProperty { object: ecad_model::ObjectRef::Project, key, .. } if crate::designations::is_list_property(key) => {
                let (_, entry, is_code) = crate::designations::parse_list_property(key)?;
                if is_code {
                    if doc.properties.iter().any(|(k, v)| crate::designations::Aspect::of_property(k).is_some() && v == entry) {
                        return None;
                    }
                    for (c, x) in &doc.devices {
                        if x.properties.iter().any(|(k, v)| crate::designations::Aspect::of_property(k).is_some() && v == entry) {
                            out.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page));
                        }
                    }
                    for (pid, x) in &doc.pages {
                        if x.properties.iter().any(|(k, v)| crate::designations::Aspect::of_property(k).is_some() && v == entry) {
                            out.insert(*pid);
                        }
                    }
                }
            }
            // Brief 044 (design 044 §11.1): a connector naming or description default is drawn by
            // the connectors that inherit it: their pages. The maker's answers are drawn nowhere.
            // Their pages come from the connector index after the commit (`Project::finish_watch`).
            Op::SetProperty { object: ecad_model::ObjectRef::Project, key, .. } if key.starts_with("connector.") => {}
            // Brief 030: strip accessory defaults are drawn nowhere (the plan page is a generated
            // page, kept by its own rule).
            Op::SetProperty { object: ecad_model::ObjectRef::Project, key, .. } if crate::accessories::strip_key_of_project(key).is_some() => {}
            // Brief 050a (design 050 §10): a style record marks the pages of its users (every page
            // when the project's choice resolves to it); the standard's header none.
            Op::SetStyle { .. } | Op::SetStandard { .. } => out.extend(crate::styles::pages_touched(doc, op)?),
            // A standard's value acts like a project-level write of its key, and changes nothing on
            // a page while the project holds its own value.
            Op::SetProperty { object: ecad_model::ObjectRef::Standard, key, before, after } => {
                let hidden = crate::defaults::find(key)
                    .is_some_and(|e| crate::defaults::resolve(doc, e, crate::defaults::Scope::project()).from == crate::defaults::Source::Project);
                if !hidden {
                    let as_project =
                        Op::SetProperty { object: ecad_model::ObjectRef::Project, key: key.clone(), before: before.clone(), after: after.clone() };
                    out.extend(pages_touched(doc, std::slice::from_ref(&as_project), plc)?);
                }
            }
            _ => return None,
        }
    }
    // Brief 052 (design 052 §8.2): a device's location and function are stored, so a box or a
    // page value changes the tag prefixes of that page only, and a moved symbol those of its own
    // page; both pages are named by the ops above. #102's widening is gone.
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

/// `pages` and `refs` together; `None` (every page) when either is.
fn touched(pages: Option<BTreeSet<PageId>>, refs: Option<BTreeSet<PageId>>) -> Option<BTreeSet<PageId>> {
    let (mut a, b) = (pages?, refs?);
    a.extend(b);
    Some(a)
}
