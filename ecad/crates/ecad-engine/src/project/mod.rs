//! The ECAD project as a CADCraft document owner.
//!
//! [`Project`] holds the authoritative [`Ledger`] and one view per page. A page that is open in the
//! engine is a CADCraft document whose drawing is a projection of the ECAD document; the
//! [`HandleMap`] relates its handles to ECAD IDs and lives only in the session.

use crate::styles::draw::DrawApply as _;
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, Weak};

use cadcraft_engine::doc::{Block, Common, Drawing, Entity, EntityKind, EntityStore, Group, Handle, Layer as CLayer, Lineweight, Text};
use cadcraft_engine::ext::{Commit, DocumentOwner, OwnerRef, Start, Verdict};
use cadcraft_engine::{EngineError, Session};

use crate::access::{Pages, PagesRef, ProjectHandle};
use crate::dev::snapshot::{LiveFacts, PageInfo};
use crate::history::event_log::{EventKind, EventLog};
use ecad_electrical::CmdError;
use ecad_model::{
    Author, CableId, ConnectionPointId, DeviceId, Document, End, Form, IdAllocator, ItemKind, Layer, LayerId, Ledger, LengthUnit, Op, Page, PageId,
    ProjectId, ProjectSettings, Pt, RepresentationId, Subject, WireId,
};
use ecad_store::MemoryBackend;
use serde_json::{Value, json};

pub use crate::cables::views::View;
use crate::convert::{ECAD_PREFIX, MARKER_LAYER, block_name, entity_for, kind_for_primitive, marker_entity};
use crate::translate;
use ecad_electrical::reports::kept::ReportModels;

mod changes;
mod handle_map;
mod open;
mod owner;
mod page_tree;
mod parts;
mod persistence;

pub use changes::Change;
use changes::Path;
pub use handle_map::{HandleMap, Mapped};
pub use owner::{LISP_REFUSED, REFUSED_COMMANDS};
pub use page_tree::{PageRow, PageView, TreeRow, tree_rows};
pub(crate) use persistence::ClipSnapshot;
pub use persistence::{Faults, MemoryStore};
pub use persistence::{Persistence, SAVE_UNAVAILABLE};

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
    pub pages: BTreeMap<PageId, PageView>,
    pub events: EventLog,
    pub(crate) clips: BTreeMap<u64, ClipSnapshot>,
    pub faults: Faults,
    /// Distance within which a point lands on a terminal.
    pub tolerance: f64,
    /// Layer used for new ECAD-drawn geometry.
    pub default_layer: LayerId,
    me: Weak<Mutex<Project>>,
    /// The devices whose stored value differs from what the last
    /// placement, move or box proposed, waiting for Move or Leave (Leave by default; any other
    /// edit leaves them).
    pub conflicts: Vec<ecad_electrical::symbols::proposals::Conflict>,
    /// What OOPS may bring back on each page document ([`crate::translate::oops`]).
    oops: crate::translate::oops::Records,
    /// The merge last prepared (its states and scope), kept while nothing it read changes
    pub(crate) merge_cache: Option<Arc<crate::history::merging::Prepared>>,
    /// Every derived view, updated by the one commit pipeline.
    pub(crate) derived: ecad_electrical::views::Views,
    /// The file side: where the project is saved and what opening reported.
    pub storage: parts::Storage,
    /// What this session shows (never in the ledger or the file).
    pub display: parts::Display,
    /// The change log and the counts that say what could differ.
    pub counts: parts::Counts,
    /// The active page's frame as last drawn.
    pub frame: parts::FrameCache,
    /// What is read from the log as it grows.
    pub(crate) indexes: parts::Indexes,
    /// This session's prompt defaults.
    pub defaults: parts::Defaults,
    /// What a command asks of the desktop app, and its answers.
    pub asks: parts::AppAsks,
    /// Who is calling, and what the call may record.
    pub caller: parts::Caller,
}

/// The derived views of `doc`, built with this engine's inputs.
pub(crate) fn views_for(doc: &Document) -> ecad_electrical::views::Views {
    let mut views = engine_views();
    views.rebuild(doc);
    views
}

/// The derived views with this engine's inputs, none built yet: for [`Views::detached`] where no
/// project is at hand.
///
/// [`Views::detached`]: ecad_electrical::views::Views::detached
pub fn engine_views() -> ecad_electrical::views::Views {
    ecad_electrical::views::Views::new(ecad_electrical::views::EngineInputs {
        look: Arc::new(crate::styles::draw::LOOK),
        registry: &ecad_drawing_style::REGISTRY,
    })
}

/// A new project's first page: titled and numbered `1`.
fn first_page() -> Result<Page, ecad_model::naming::text::TextError> {
    let mut p = Page::titled("1")?;
    p.number = ecad_model::PageNo::parse("1");
    Ok(p)
}

/// What this program records about itself in a file header and a writer lock.
pub fn writer() -> String {
    format!("ecad {} (cadcraft {})", env!("CARGO_PKG_VERSION"), crate::UPSTREAM_COMMIT.get(..7).unwrap_or_default())
}

/// A project's name from its file path: the file name without `.ecad`.
fn file_stem(path: &str) -> String {
    std::path::Path::new(path).file_stem().map_or_else(|| path.to_string(), |s| s.to_string_lossy().into_owned())
}

/// Milliseconds since the epoch (`None` for a clock set before it): the wall clock the engine
/// stamps branches and revisions with, and the app's default host reads until the program
/// installs its own.
pub fn now_ms() -> Option<u64> {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// A seed for IDs that differs for every call: a per-process counter mixed with the clock and,
/// the process id. Two projects created in the same millisecond still get different ID streams.
pub fn session_seed() -> u64 {
    static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = u64::from(std::process::id());
    // SplitMix64 finaliser over the combined inputs, so nearby values give unrelated seeds.
    let mut z = now_ms().unwrap_or(0x5EED) ^ pid.rotate_left(40) ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The reserved layer of cable fan views (`ecad.cable`, plots).
fn cables_layer() -> CLayer {
    let mut l = CLayer::new(crate::cables::views::LAYER);
    l.color = cadcraft_engine::doc::color::Color::Index(crate::cables::views::CABLE_ACI);
    l
}

/// One computed part of a cable's drawing as an entity on the reserved layer:
/// a line for two points, else a polyline; an arc; a text. Trunk and spine carry the cable
/// lineweight, bends, tails and frames the wire lineweight, text the layer's. A text takes its
/// height and style from `draw`: a reference end's text is a reference, every other
/// one cable text.
pub(crate) fn part_entity(draw: &crate::styles::draw::DrawStyles, part: &ecad_electrical::cables::layout::Part, handle: Handle) -> Entity {
    use ecad_electrical::cables::layout::{Role, Shape, Weight};
    use ecad_model::look::TextKind;
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
            height: draw.style(text_kind).height,
            rotation: ecad_model::geometry::units::Radians(*rotation),
            value: value.clone(),
            halign: Default::default(),
            valign: Default::default(),
        }),
    };
    let lineweight = match part.weight {
        Weight::Cable => Lineweight::Mm100(ecad_electrical::cables::layout::CABLE_WEIGHT),
        Weight::Wire => Lineweight::Mm100(ecad_electrical::cables::words::WIRE_WEIGHT),
        Weight::Text => Lineweight::ByLayer,
    };
    let color = part.color.map_or(cadcraft_engine::doc::color::Color::ByLayer, cadcraft_engine::doc::color::Color::Index);
    let mut e = Entity { handle, common: Common { layer: crate::cables::views::LAYER.into(), lineweight, color, ..Common::default() }, kind };
    if matches!(part.shape, Shape::Text { .. }) {
        draw.apply(&mut e, text_kind);
    }
    e
}

/// One part of a device cross-reference as an entity on the tags layer: a text, or a
/// line (the table's bar at the wire lineweight).
/// A text part is drawn in its kind's text style (font, weight, colour), as every
/// other derived text is.
pub(crate) fn device_reference_entity(
    draw: &crate::styles::draw::DrawStyles, part: &ecad_electrical::references::devices::table::Part, handle: Handle,
) -> Entity {
    let mut e = match &part.shape {
        ecad_electrical::references::devices::table::Shape::Text { kind, at, value, height, halign, valign, rotation } => {
            let mut e = crate::devices::draw::text_entity_with(
                value,
                *at,
                *height,
                crate::references::draw::devices::engine_halign(*halign),
                crate::references::draw::devices::engine_valign(*valign),
                handle,
                ecad_electrical::devices::TAG_LAYER,
            );
            // A continuation's reference reads along its wire; every other part
            // keeps 0, so its entity is unchanged.
            if let EntityKind::Text(t) = &mut e.kind {
                t.rotation = *rotation;
            }
            draw.apply(&mut e, *kind);
            e
        }
        ecad_electrical::references::devices::table::Shape::Line(a, b) => Entity {
            handle,
            common: Common { layer: ecad_electrical::devices::TAG_LAYER.into(), ..Common::default() },
            kind: kind_for_primitive(&ecad_model::Primitive::Line { a: *a, b: *b }),
        },
    };
    if part.bar {
        e.common.lineweight = Lineweight::Mm100(ecad_electrical::cables::words::WIRE_WEIGHT);
    }
    if let Some(aci) = part.aci {
        e.common.color = cadcraft_engine::doc::color::Color::Index(aci);
    }
    e
}

/// One segment of a jumper bar's line, dashed on the jumper bar layer.
pub(crate) fn jumper_bar_entity(part: &ecad_electrical::terminal_strips::jumper_bars::JumperBarPart, handle: Handle) -> Entity {
    Entity {
        handle,
        common: Common {
            layer: ecad_electrical::terminal_strips::jumper_bars::LAYER.into(),
            linetype: ecad_electrical::locations::LINETYPE.into(),
            lineweight: Lineweight::Mm100(ecad_electrical::cables::words::WIRE_WEIGHT),
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
    // The starter symbols' terminals lie on a 5 mm grid (`ecad.symbol-grid`): snap
    // and grid spacing of an ECAD page follow it.
    let grid = cadcraft_engine::doc::HVal::Point(cadcraft_engine::geom::Vec3::new(
        ecad_electrical::symbols::SYMBOL_GRID_MM,
        ecad_electrical::symbols::SYMBOL_GRID_MM,
        0.0,
    ));
    d.header.set("SNAPUNIT", grid.clone());
    d.header.set("GRIDUNIT", grid);
    // An ECAD page has no user coordinate system to show: no UCS icon (UCSICON off).
    d.header.set("UCSICON", cadcraft_engine::doc::HVal::Int(0));
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
    // The built-in `Free` text style; the page could not be read.
    let free = ecad_drawing_style::text::text_style(&ecad_model::Document::default(), ecad_model::look::TextKind::Free);
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
/// handles from `prev` for objects that are still there, in the colours of the paper sheet. Pages
/// drawn from the report models use `reports` ([`crate::pages::generated::parts_of`]).
///
/// # Errors
/// An item that cannot be drawn (missing layer or definition).
pub fn project_page(
    doc: &Document, page: PageId, base: &Drawing, prev: &HandleMap, reports: Option<&ReportModels>,
) -> Result<(Drawing, HandleMap), String> {
    project_page_with(doc, page, base, prev, &BTreeSet::new(), 0.0, reports, &ecad_model::palette::sheet::PAPER)
}

/// [`project_page`] with cable views: each cable in `fan` whose fan can be drawn on
/// the page is shown as its fan ([`crate::cables::views::fan_on_page`], with tolerance `tol`) on layer
/// [`crate::cables::views::LAYER`] instead of its member routes and slashes. Everything else is projected
/// exactly as by [`project_page`], in the colours of sheet `colours` (paper or dark). Both draw
/// each cable reference's derived text on that layer too
/// ([`ecad_electrical::references::cables::markers_on_page`]).
///
/// # Errors
/// As [`project_page`].
#[allow(clippy::too_many_arguments)]
pub fn project_page_with(
    doc: &Document, page: PageId, base: &Drawing, prev: &HandleMap, fan: &BTreeSet<CableId>, tol: f64, reports: Option<&ReportModels>,
    colours: &'static ecad_model::palette::Sheet,
) -> Result<(Drawing, HandleMap), String> {
    project_page_framed(doc, page, base, prev, fan, tol, &crate::pages::generated::parts_of(doc, page, reports), colours)
}

/// [`project_page_with`] with the sheet frame's parts already worked out (the project keeps them
/// between edits that do not change them).
///
/// # Errors
/// As [`project_page`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn project_page_framed(
    doc: &Document, page: PageId, base: &Drawing, prev: &HandleMap, fan: &BTreeSet<CableId>, tol: f64, frame: &[crate::pages::frame::Part],
    colours: &'static ecad_model::palette::Sheet,
) -> Result<(Drawing, HandleMap), String> {
    project_page_shown(doc, page, base, prev, fan, tol, frame, false, colours)
}

/// [`project_page_with`] as the page window shows it: with Show Hidden Text, the project's pending
/// flag (the frame's Rev box and the cover show the pending marker as the project draws them) and
/// only sheet `sheet` of a generated page ([`crate::pages::generated::shown_parts`]).
///
/// # Errors
/// As [`project_page`].
#[allow(clippy::too_many_arguments)]
pub fn project_page_pending(
    doc: &Document, page: PageId, base: &Drawing, prev: &HandleMap, fan: &BTreeSet<CableId>, tol: f64, hidden_text: bool, pending: bool,
    sheet: usize, reports: Option<&ReportModels>, colours: &'static ecad_model::palette::Sheet,
) -> Result<(Drawing, HandleMap), String> {
    let frame = crate::pages::generated::shown_parts(doc, page, reports, pending, sheet);
    project_page_shown(doc, page, base, prev, fan, tol, &frame, hidden_text, colours)
}

/// [`project_page_framed`] with Show Hidden Text (see [`project_page_pending`]).
///
/// # Errors
/// As [`project_page`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn project_page_shown(
    doc: &Document, page: PageId, base: &Drawing, prev: &HandleMap, fan: &BTreeSet<CableId>, tol: f64, frame: &[crate::pages::frame::Part],
    hidden_text: bool, colours: &'static ecad_model::palette::Sheet,
) -> Result<(Drawing, HandleMap), String> {
    project_page_kept(doc, page, base, prev, fan, tol, frame, hidden_text, None, colours)
}

/// [`project_page_shown`] with the kept PLC model, when built.
///
/// # Errors
/// As [`project_page`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn project_page_kept(
    doc: &Document, page: PageId, base: &Drawing, prev: &HandleMap, fan: &BTreeSet<CableId>, tol: f64, frame: &[crate::pages::frame::Part],
    hidden_text: bool, plc: Option<&ecad_electrical::plc::kept::Plc>, colours: &'static ecad_model::palette::Sheet,
) -> Result<(Drawing, HandleMap), String> {
    let mut d = base.clone();
    for l in doc.layers().values() {
        if d.layer(&l.name).is_none() {
            d.layers.push(CLayer::new(&l.name));
        }
    }
    if d.layer(MARKER_LAYER).is_none() {
        let mut l = CLayer::new(MARKER_LAYER);
        l.plot = false;
        d.layers.push(l);
    }
    if d.layer(crate::cables::views::LAYER).is_none() {
        d.layers.push(cables_layer());
    }
    if d.layer(ecad_electrical::devices::TAG_LAYER).is_none() {
        d.layers.push(CLayer::new(ecad_electrical::devices::TAG_LAYER));
    }
    if d.layer(ecad_electrical::wires::LABEL_LAYER).is_none() {
        let mut l = CLayer::new(ecad_electrical::wires::LABEL_LAYER);
        l.color = cadcraft_engine::doc::color::Color::Index(ecad_electrical::wires::LABEL_ACI);
        d.layers.push(l);
    }
    if d.layer(ecad_electrical::pages::frames::LAYER).is_none() {
        d.layers.push(crate::pages::frame::layer());
    }
    crate::locations::draw::add_linetype(&mut d);
    if d.layer(ecad_electrical::terminal_strips::jumper_bars::LAYER).is_none() {
        d.layers.push(CLayer::new(ecad_electrical::terminal_strips::jumper_bars::LAYER));
    }
    if d.layer(ecad_electrical::pages::generated::LAYER).is_none() {
        let mut l = CLayer::new(ecad_electrical::pages::generated::LAYER);
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
        let def = doc.definitions().get(def_id).ok_or_else(|| format!("definition {def_id} is missing"))?;
        let mut b = Block::new(&name);
        for g in &def.graphics {
            let h = d.new_handle();
            let common = cadcraft_engine::doc::Common { layer: "0".into(), color: cadcraft_engine::doc::color::Color::ByBlock, ..Default::default() };
            b.entities.push(Entity { handle: h, common, kind: kind_for_primitive(g) });
        }
        for t in &def.connection_points {
            let h = d.new_handle();
            b.entities.push(marker_entity(t.at, h));
        }
        d.blocks.insert(name, Arc::new(b));
    }
    let fans = crate::cables::views::drawings_on_page(doc, page, fan, tol);
    let hidden: BTreeSet<ecad_model::ItemId> = fans.values().flat_map(|f| f.hidden.iter().copied()).collect();
    let mut map = HandleMap::default();
    let mut ents: Vec<Entity> = Vec::new();
    for (id, item) in doc.items_on(page).filter(|(id, _)| !hidden.contains(id)) {
        let h = prev.item(id).unwrap_or_else(|| d.new_handle());
        let e = entity_for(doc, item, h).ok_or_else(|| format!("item {id} cannot be drawn"))?;
        ents.push(e);
        map.insert_item(id, h);
    }
    // The project's wire categories, read once for every wire of the page.
    let draws = ecad_electrical::styles::wire::WireDraws::new(doc);
    // Wires and cores in their colours, two-colour ones with dashes (wires.rs).
    crate::wires::draw::colour_routes(doc, page, &mut d, &mut ents, &mut map, prev, colours, &draws);
    // Wire routes in their category's line type and weight.
    crate::wires::draw::style_routes(doc, page, &draws, &mut ents, |i| map.item(i));
    // Location box outlines in their location style.
    crate::locations::draw::style_boxes(doc, page, &mut ents, |i| map.item(i), colours);
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
    // The project's text styles, for every derived text of the page.
    let draw = crate::styles::draw::DrawStyles::new(doc, colours);
    for (k, f) in &fans {
        map.quiet.extend(f.quiet.iter().copied());
        for (i, part) in f.parts.iter().enumerate() {
            // A reference end's text (its live text) is mapped to its
            // reference, so hover, Go to Reference and erasing it act on the reference.
            if part.role == ecad_electrical::cables::layout::Role::ReferenceText
                && let Some(r) = part.reference
            {
                let h = prev.ref_text(r).unwrap_or_else(|| d.new_handle());
                ents.push(part_entity(&draw, part, h));
                map.insert_ref_text(r, h);
                continue;
            }
            let index = u32::try_from(i).map_err(|_| "a cable drawing has too many parts")?;
            let h = prev.fan(*k, index).unwrap_or_else(|| d.new_handle());
            ents.push(part_entity(&draw, part, h));
            map.insert_part(*k, index, h, f.is_fan(), part.core, part.reference);
        }
    }
    // Device tags and terminal labels: derived on every draw, never stored, in the
    // project's text styles.
    crate::styles::draw::install_draw(&mut d, &draw);
    if hidden_text {
        crate::styles::draw::add_hidden_layer(&mut d);
    }
    for t in crate::devices::draw::all_texts_on_page_with(doc, page, plc) {
        if !t.shown && !hidden_text {
            continue;
        }
        // An object's own text style in a font of its own.
        if let Some(f) = t.text.own.as_ref().and_then(|o| o.family.as_deref()) {
            crate::styles::draw::install_family(&mut d, doc, f);
        }
        let h = prev.tag_text(t.text.rep, t.text.of).unwrap_or_else(|| d.new_handle());
        let mut e = crate::devices::draw::text_entity(&draw, &t.text, h);
        if !t.shown {
            crate::styles::draw::dim_hidden(&mut e);
        }
        ents.push(e);
        map.insert_tag_text(t.text.rep, t.text.of, h);
    }
    // Device cross-references: texts under contacts and tables under heads, derived
    // on every draw of the page, never stored.
    for part in crate::references::draw::devices::parts_on_page(doc, page) {
        let h = prev.reference(part.rep, part.index).unwrap_or_else(|| d.new_handle());
        let mut e = device_reference_entity(&draw, &part, h);
        // A continuation's other-end text in the sheet's Ink 2.
        crate::continuations::draw::colour(doc, &part, &mut e, colours);
        ents.push(e);
        map.insert_device_reference(part.rep, part.index, h);
    }
    // PLC card frames: derived on every draw, never stored; mapped like the device reference parts.
    for part in crate::plc::draw::card::parts_on_page(doc, page) {
        let h = prev.reference(part.rep, part.index).unwrap_or_else(|| d.new_handle());
        ents.push(device_reference_entity(&draw, &part, h));
        map.insert_device_reference(part.rep, part.index, h);
    }
    // Jumper bar lines: derived on every draw, never stored.
    let mut seen: BTreeMap<(DeviceId, u32), u32> = BTreeMap::new();
    for part in ecad_electrical::terminal_strips::jumper_bars::parts_on_page(doc, page) {
        let i = seen.entry((part.strip, part.n)).or_default();
        let h = prev.jumper_bar(part.strip, part.n, *i).unwrap_or_else(|| d.new_handle());
        ents.push(jumper_bar_entity(&part, h));
        map.insert_jumper_bar(part.strip, part.n, *i, h);
        *i += 1;
    }
    // T dots: where a route ends on another route, derived on every draw of the
    // page, never stored; in the T route's own colour, layer and weight. A route hidden by a fan
    // view draws none.
    for (r, end, at) in ecad_electrical::wires::routes::tees_on_page(doc, page) {
        let Some((_, item, _)) = ecad_electrical::wires::routes::route_item(doc, r) else { continue };
        if hidden.contains(&item) {
            continue;
        }
        let common = map
            .item(item)
            .and_then(|h| ents.iter().find(|e| e.handle == h))
            .map(|e| Common { linetype: "Continuous".into(), ..e.common.clone() })
            .unwrap_or_default();
        for (ring, radius) in (0u8..).zip(ecad_electrical::wires::TEE_DOT_RADII) {
            let h = prev.tee(r, end, ring).unwrap_or_else(|| d.new_handle());
            let kind = kind_for_primitive(&ecad_model::Primitive::Circle { center: at, radius });
            ents.push(Entity { handle: h, common: common.clone(), kind });
            map.insert_tee(r, end, ring, h);
        }
    }
    // Location box labels: derived on every draw, never stored.
    for (rep, value, at) in ecad_electrical::locations::labels_on_page(doc, page) {
        let h = prev.tag_text(rep, ecad_electrical::devices::texts::TextOf::Tag).unwrap_or_else(|| d.new_handle());
        if let Some(f) = ecad_drawing_style::catalogues::location::label_family(doc, rep) {
            crate::styles::draw::install_family(&mut d, doc, &f);
        }
        ents.push(crate::locations::draw::styled_label_entity(doc, &draw, rep, &value, at, h));
        map.insert_tag_text(rep, ecad_electrical::devices::texts::TextOf::Tag, h);
    }
    // Looks-connected marks: derived with the page, after its quiet ends.
    map.draws = ecad_electrical::wires::joining::looks_connected_on_page(doc, page, &map.quiet);
    // Wire labels: derived on every draw of the page, never stored.
    // A cable in fan view hides its members' routes, and their labels with them:
    // display only, nothing is written.
    for l in crate::wires::draw::labels_on_page_with(doc, page, &hidden, &draws) {
        if let Some(f) = l.own.as_ref().and_then(|o| o.family.as_deref()) {
            crate::styles::draw::install_family(&mut d, doc, f);
        }
        let h = prev.label(l.rep).unwrap_or_else(|| d.new_handle());
        ents.push(crate::wires::draw::label_entity(&draw, &l, h));
        map.insert_label(l.rep, l.conductor, h);
    }
    ents.sort_by_key(|e| e.handle);
    let mut store = EntityStore::new();
    // The sheet frame and title block: derived, never stored, and drawn first so
    // everything else lies on top of it (picks find the drawing, not the frame).
    // The frame style's line colour, on the frame's own lines (ink otherwise).
    let frame_rgb = crate::pages::frame::line_colour(doc, page, colours);
    for (i, part) in frame.iter().enumerate() {
        let index = u32::try_from(i).map_err(|_| "the frame has too many parts")?;
        let h = prev.frame_part(index).unwrap_or_else(|| d.new_handle());
        store.push(crate::pages::frame::styled_entity(part, h, frame_rgb));
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
        // A page with PLC channel cells reads its addresses from the
        // kept PLC model, built on its first show.
        if self.kept_reports().is_none() && ecad_electrical::plc::page_has_cells(self.ledger.document(), page) {
            self.reports();
        }
        let plc = self.kept_reports().map(|o| &o.plc);
        project_page_kept(
            self.ledger.document(),
            page,
            base,
            prev,
            &self.display.views,
            self.tolerance,
            &parts,
            self.display.show_hidden_text,
            plc,
            self.display.sheet,
        )
    }

    /// The frame parts of `page` (its shown sheet), from the kept copy when nothing it shows can
    /// have changed.
    fn frame_parts(&mut self, page: PageId) -> Arc<Vec<crate::pages::frame::Part>> {
        let sheet = self.sheet_index(page);
        if let Some((p, s, parts)) = &self.frame.drawn
            && *p == page
            && *s == sheet
            && !self.frame.stale
        {
            return parts.clone();
        }
        // A BOM page's sheet count (and so every page's Total) needs the report models;
        // once built they track their pages themselves.
        if self.kept_reports().is_none() && ecad_electrical::pages::generated::has_bom_page(self.ledger.document()) {
            self.reports();
        }
        if !self.frame.pending_known {
            self.refresh_pending();
        }
        let doc = self.ledger.document();
        let (input, total) = crate::pages::generated::input_of(doc, page, self.kept_reports());
        let input = input.with_pending(doc, self.frame.pending);
        self.frame.stale = false;
        let generated = ecad_electrical::pages::generated::kind_of(doc, page).is_some();
        if let Some((p, s, parts)) = &self.frame.drawn
            && *p == page
            && *s == sheet
            && self.derived.frame().shown().is_some_and(|(q, old)| q == page && *old == input.0)
            && !generated
        {
            return parts.clone();
        }
        self.counts.frame_builds += 1;
        let parts = Arc::new(crate::pages::generated::sheet_parts(doc, page, &input, self.kept_reports(), total, sheet));
        self.frame.drawn = Some((page, sheet, parts.clone()));
        self.derived.frame_mut().show(page, input.0);
        parts
    }

    /// The view `cable` is shown in.
    pub fn view_of(&self, cable: CableId) -> View {
        if self.display.views.contains(&cable) { View::Fan } else { View::Traditional }
    }

    /// Show `cable` in `view`: presentation only, so no transaction, no head move, no
    /// unsaved change. Every open page is redrawn through the same path as after an undo, and the
    /// fan-view set is remembered in the display's `view_settings` ([`Project::display`]). Returns whether the view changed and,
    /// when it changed, whether it was remembered (else why not).
    pub(crate) fn set_view(&mut self, pg: &mut Pages<'_>, cable: CableId, view: View) -> (bool, Result<(), String>) {
        let changed = match view {
            View::Fan => self.display.views.insert(cable),
            View::Traditional => self.display.views.remove(&cable),
        };
        if !changed {
            return (false, Ok(()));
        }
        // IDs no longer at the head are dropped from what is remembered.
        let doc = self.ledger.document();
        self.display.views.retain(|k| doc.cables().contains_key(k));
        // A view is drawn on every page showing the cable: the active page is redrawn, every
        // other page when it becomes active.
        self.record_change(None, ecad_electrical::devices::device_list::DeviceTouch::default(), Some(BTreeSet::new()), false);
        self.redraw_window(pg, Some("cableview"));
        (true, self.display.view_settings.store(self.id, &self.display.views))
    }

    /// Turn Show Hidden Text on or off: display only, like a cable view:
    /// no transaction, no head move, no unsaved change. The open page is redrawn now, the others
    /// when they become active. Returns whether it changed.
    pub(crate) fn set_hidden_text(&mut self, pg: &mut Pages<'_>, on: bool) -> bool {
        if self.display.show_hidden_text == on {
            return false;
        }
        self.display.show_hidden_text = on;
        self.record_change(None, ecad_electrical::devices::device_list::DeviceTouch::default(), Some(BTreeSet::new()), false);
        self.redraw_window(pg, Some("hiddentext"));
        true
    }

    fn owner_ref(&self) -> Option<OwnerRef> {
        let strong = self.me.upgrade()?;
        let r: OwnerRef = strong;
        Some(r)
    }

    /// Whether the project holds design work that is not saved: head versus persisted position,
    /// or, with no persistence, head versus the position it was created at. A new file
    /// project that was never edited (only its creation transaction) and never written has
    /// nothing to lose; once a file exists, head versus persisted applies strictly.
    pub fn unsaved(&self) -> bool {
        self.unsaved_transactions() > 0
    }

    /// How many transactions the unsaved state covers (0 when nothing is unsaved).
    pub fn unsaved_transactions(&self) -> u64 {
        let from = match self.storage.persistence {
            Persistence::TestStore => self.ledger.persisted(),
            Persistence::File if self.storage.file.bound() => self.ledger.persisted(),
            Persistence::File => self.ledger.persisted().max(self.storage.baseline),
            Persistence::Unavailable => self.storage.baseline,
        };
        self.ledger.head().saturating_sub(from)
    }

    /// The page shown by engine document `uid`.
    pub fn page_of(&self, uid: u64) -> Option<PageId> {
        self.pages.iter().find(|(_, v)| v.uid == Some(uid)).map(|(p, _)| *p)
    }

    /// How many pages are open.
    /// The functions and locations list with its counts, as `designations` answers (Settings ▸
    /// Project info). Catches the use index up first.
    pub fn designation_rows(&mut self) -> serde_json::Value {
        self.indexes.designation_uses.catch_up(&self.ledger);
        ecad_electrical::locations::commands::designations::rows(self.ledger.document(), &self.indexes.designation_uses, None)
    }

    pub fn open_pages(&self) -> usize {
        self.pages.values().filter(|v| v.uid.is_some()).count()
    }

    /// The page window's title: `demo - page 2: Field`.
    fn page_title(&self, page: PageId) -> String {
        let doc = self.ledger.document();
        let t = doc.pages().get(&page).map(|p| p.title.to_string()).unwrap_or_default();
        format!("{} - page {}: {t}", self.name, ecad_electrical::references::cables::page_number(doc, page))
    }

    /// The page shown in the page window (the active page), if the window is open.
    pub fn active_page(&self) -> Option<PageId> {
        self.display.active.filter(|p| self.pages.get(p).is_some_and(|v| v.uid.is_some()))
    }

    /// The page window: the active page and the engine document showing it.
    pub fn window(&self) -> Option<(PageId, u64)> {
        let page = self.active_page()?;
        Some((page, self.pages.get(&page)?.uid?))
    }

    /// The Project panel's page rows: every page in book order with the
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

    /// The Pages tree: folders and pages in book order, each with its depth.
    /// Reads pages and folders only: O(pages + folders).
    pub fn tree_rows(&self) -> Vec<TreeRow> {
        tree_rows(self.ledger.document(), |id| self.pages.get(&id).is_some_and(|v| v.unavailable))
    }

    /// Bring the tag numbering up to date with the log.
    fn catch_up_tags(&mut self) {
        self.indexes.tag_high.catch_up(&self.ledger);
    }

    /// Record one change and the pages it could change.
    ///
    /// The tag numbering ([`ecad_electrical::devices::tags::TagHigh`]) reads the transactions this change added,
    /// so reads (prompts, previews, pick lists) never read the log. The Pages tree's version moves
    /// only with a change of its update map (`tree`, or pages added or removed).
    /// Work out the pending flag again: O(1) on an ordinary edit, the walk
    /// stops at the first decisive transaction. A flip redraws the frame.
    pub(crate) fn refresh_pending(&mut self) {
        let now = ecad_electrical::releases::revisions::is_pending(&self.ledger);
        if self.frame.pending_known && now != self.frame.pending {
            self.frame.stale = true;
        }
        self.frame.pending = now;
        self.frame.pending_known = true;
    }

    /// Make `page` the active page: it is projected into the project's one page window,
    /// which is opened if it is closed. The page shown before keeps its view for this session; a
    /// page shown before gets its view back, one shown for the first time is zoomed to its
    /// extents. A page already active is only brought to the front.
    ///
    /// # Errors
    /// Unknown page or projection failure; the page shown before stays shown.
    pub(crate) fn open_page(&mut self, pg: &mut Pages<'_>, page: PageId) -> Result<u64, String> {
        if !self.ledger.document().pages().contains_key(&page) {
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
                self.display.page_views.insert(old, left);
                // What OOPS kept belongs to the page drawn there before.
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
        let remembered = self.display.page_views.get(&page).cloned();
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
            self.counts.page_list_version += 1;
        }
        self.display.active = Some(page);
        self.events.record(EventKind::PageOpened, None, self.ledger.head(), Some(page), title);
        self.sync_saved(pg);
        Ok(uid)
    }

    /// The sheet (0-based) `page` was last asked to show, before clamping to its sheets.
    pub(crate) fn sheet_index(&self, page: PageId) -> usize {
        self.display.sheets.get(&page).copied().unwrap_or(0)
    }

    /// The sheet (0-based) the page window shows of `page` and how many it has: one for an
    /// ordinary page; a generated page's count from the kept report models (one before they are
    /// built).
    pub fn shown_sheet(&self, page: PageId) -> (usize, usize) {
        let n = ecad_electrical::pages::generated::sheets_of(self.ledger.document(), page, self.kept_reports()).max(1);
        (self.sheet_index(page).min(n - 1), n)
    }

    /// Make sheet `sheet` (0-based, clamped to its sheets) the one `page` shows in the page
    /// window, redrawing the window when it shows `page` and the sheet changed. Display only:
    /// nothing is recorded, and the view is kept, since every sheet lies at the origin.
    pub(crate) fn set_sheet(&mut self, pg: &mut Pages<'_>, page: PageId, sheet: usize) {
        if ecad_electrical::pages::generated::kind_of(self.ledger.document(), page).is_some() && self.kept_reports().is_none() {
            self.reports();
        }
        let (shown, n) = self.shown_sheet(page);
        let sheet = sheet.min(n - 1);
        if sheet == shown {
            return;
        }
        if sheet == 0 {
            self.display.sheets.remove(&page);
        } else {
            self.display.sheets.insert(page, sheet);
        }
        if self.active_page() == Some(page) {
            self.redraw_window(pg, Some("openpage"));
        }
        self.counts.page_list_version += 1;
    }

    /// The sheet (0-based) of `page` that point `at` of its side-by-side layout lies on (as Print
    /// Book draws it, [`ecad_electrical::pages::generated::sheet_at`]) and the point on that sheet
    /// as the page window shows it: what a go-to opens. An ordinary page's point is unchanged.
    pub fn sheet_at(&mut self, page: PageId, at: Pt) -> (usize, Pt) {
        if ecad_electrical::pages::generated::kind_of(self.ledger.document(), page).is_some() && self.kept_reports().is_none() {
            self.reports();
        }
        ecad_electrical::pages::generated::sheet_at(self.ledger.document(), page, self.kept_reports(), at)
    }

    fn take_projection_fault(&mut self) -> bool {
        if self.faults.fail_projections > 0 {
            self.faults.fail_projections -= 1;
            true
        } else {
            false
        }
    }

    /// The report models, built whole on their first show and kept after that
    /// by the views' pipeline (`ecad_electrical::views::ReportsView`).
    pub fn reports(&mut self) -> &ecad_electrical::reports::kept::ReportModels {
        self.derived.reports(self.ledger.document())
    }

    /// The report models, if shown once.
    pub fn kept_reports(&self) -> Option<&ecad_electrical::reports::kept::ReportModels> {
        self.derived.kept_reports()
    }

    /// Forget the report models: they are built again on their next show.
    pub fn forget_reports(&mut self) {
        self.derived.forget_reports();
    }

    /// The Checks list, built whole on its first show and kept after that by the
    /// views' pipeline (`ecad_electrical::views::ChecksView`).
    pub fn checks(&mut self) -> &ecad_electrical::checks::kept::Checks {
        self.derived.checks(self.ledger.document(), self.tolerance)
    }

    /// The wire and cable rating results, built on their first show and kept after that by the
    /// views' pipeline (`ecad_electrical::views::RatingView`).
    pub fn rating(&mut self) -> &ecad_electrical::circuits::kept::Kept {
        self.derived.rating(self.ledger.document())
    }

    /// The Checks list, if shown once.
    pub fn kept_checks(&self) -> Option<&ecad_electrical::checks::kept::Checks> {
        self.derived.kept_checks()
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
        // Worked out from scratch, so the check does not trust the kept models.
        let reports = self.derived.report_models(doc);
        let pages = doc
            .page_order()
            .iter()
            .map(|p| {
                let view = self.pages.get(p);
                let st = view.and_then(|v| v.uid).and_then(|u| pg.doc(u));
                let matches = match (view, st) {
                    (Some(v), Some(st)) => Some(
                        project_page_pending(
                            doc,
                            *p,
                            &st.doc,
                            &v.map,
                            &self.display.views,
                            self.tolerance,
                            self.display.show_hidden_text,
                            self.frame.pending,
                            self.sheet_index(*p),
                            reports.as_ref(),
                            self.display.sheet,
                        )
                        .map(|(d, m)| d.model == st.doc.model && m == v.map && v.map.is_consistent())
                        .unwrap_or(false),
                    ),
                    _ => None,
                };
                PageInfo {
                    id: *p,
                    title: doc.pages().get(p).map(|x| x.title.to_string()).unwrap_or_default(),
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
    /// See [`crate::dev::snapshot::snapshot`].
    pub fn snapshot(&self, pg: &PagesRef<'_>, at: Option<u64>) -> Result<Value, String> {
        let facts = self.live_facts(pg);
        crate::dev::snapshot::snapshot(&self.name, &self.ledger, at, &self.events, at.is_none().then_some(&facts)).map_err(|e| e.to_string())
    }

    /// Positions of attached route ends that follow the dragged terminals (drag preview, M2).
    fn following_routes(&self, page: PageId, moved: &dyn Fn(ConnectionPointId) -> Option<Pt>) -> Vec<cadcraft_engine::doc::EntityKind> {
        let doc = self.ledger.document();
        let mut out = Vec::new();
        for r in doc.routes_on(page) {
            let Some(route) = doc.routes().get(&r) else { continue };
            let mut points = route.points.clone();
            let mut any = false;
            for (end, landed) in End::BOTH.into_iter().zip(route.ends) {
                let slot = if end == End::A { points.first_mut() } else { points.last_mut() };
                if let (Some(to), Some(p)) = (landed.point().and_then(moved), slot) {
                    *p = to;
                    any = true;
                }
            }
            if any {
                out.push(kind_for_primitive(&ecad_model::Primitive::Polyline { vertices: points, closed: false }));
            }
        }
        out
    }
}
