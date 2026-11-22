//! Click-based forms of the ECAD commands (briefs 008, 004 and 005), and what the drawing shows
//! about ECAD objects: terminal and wire-end marks, cable slash crossing dots, the capture of a
//! terminal under the cursor, and the hover description (with a cable's view).
//!
//! # Semantics
//! - Each interactive form is CADCraft's prompt machine (`Interactive`) on the command's existing
//!   `CommandSpec`. It only collects and resolves values; every commit runs the command's **JSON
//!   form** through `Session::execute` with those values, so it records exactly the transaction
//!   the JSON form records (one per placed object). The machine never edits the drawing itself.
//! - A machine's `input` runs while the session has taken the command out of `running`, so the
//!   JSON form commits there exactly as at `Command:`; the engine's commit at the end of the
//!   command then finds nothing new. Esc keeps what was committed and drops the pending input.
//! - A terminal is *captured* when the cursor is within the APERTURE (in pixels) of a terminal
//!   shown on the page; typed points are taken literally (the JSON form's own end resolution
//!   still applies to them).
//! - The project is read with `try_lock` for prompts and previews, and released before any
//!   commit: a busy project gives a plain prompt and no preview, never a deadlock.
//!
//! This module adds no ECAD meaning: no numbering beyond the default terminal name, no new
//! transaction kinds, nothing stored. It contains no UI toolkit code.

// Brief 047: Reports ▸ BOM ▸ + Item.
// Brief 041: what is under the cursor (the wheel) and the running tool's intent.

use ecad_model::naming::keys::PropsExt;
use std::collections::BTreeSet;

use cadcraft_engine::doc::color::Color as CColor;
use cadcraft_engine::doc::{Circle, Entity, EntityKind, Handle, Line, Text};
use cadcraft_engine::ext::{Mark, MarkShape};
use cadcraft_engine::geom::{Vec2, Vec3};
use cadcraft_engine::{EngineError, Session};
use ecad_model::connectivity::derive::connection_point_positions;
use ecad_model::{CableId, ConnectionPointId, DeviceId, Document, End, Form, ItemKind, PageId, Primitive, Pt, RouteEnd, RouteId, Subject, WireId};

use crate::access::{owner_of_active, try_lock};
use crate::cables::views::{FanOnPage, View, fan_on_page};
use crate::project::{Mapped, Project};
use crate::translate::connection_point_at;
pub use ecad_electrical::cables::words::{MAX_COORD, MAX_HOVER_LINES, OUT_OF_RANGE};
use ecad_electrical::terminal_strips::terminal_blocks::short;
pub use ecad_electrical::terminal_strips::terminal_blocks::{connection_point_name, position_label};

pub(crate) use crate::connectors::interact::connector;
pub(crate) use crate::continuations::interact::contcaption;
pub(crate) use crate::devices::interact::{devicedelete, deviceprops, place};
pub(crate) use crate::locations::interact::{functionbox, locationbox};
pub(crate) use crate::properties::forms::{addconnectionpoint, cablecore, cablefan, cableslash, cableview, connect, convert, disconnect, wire};
pub(crate) use crate::references::interact::cables::{cablecontinue, cablegoto, cableref};
pub(crate) use crate::references::interact::devices::mirrormove;
pub(crate) use crate::reports::interact::bomitem;
pub(crate) use crate::terminal_strips::interact::accessories::{accessory, jumper};
pub(crate) use crate::terminal_strips::interact::terminal_blocks::{bridge, terminal_blocks};
pub(crate) use crate::wires::interact::{joinwires, renumberwires, wirenumber};

/// Refusal on a drawing that is not an ECAD page (design §3.4).
pub const NOT_OWNED: &str = "this page is not owned by an ECAD project";
/// Refusal while a paper-space layout is shown: ECAD objects live in the page's model space.
pub const PAPER_SPACE: &str = "Switch to the Model tab to draw ECAD objects.";

/// Whether the Model tab is shown: not a paper-space layout, nor a viewport of one (where
/// `Session::space` reports model space but the canvas shows the sheet).
pub(crate) fn model_space(s: &Session) -> bool {
    s.layout_space() == cadcraft_engine::doc::Space::Model
}

/// Canvas colours (design tokens, ACI on the dark canvas).
pub(crate) const CONNECTION_POINT_ACI: u8 = 4;
pub(crate) const ATTACHED_ACI: u8 = 3;
/// The owner chose a neutral colour for loose ends, not red (brief 008 answer Q3): light grey.
pub(crate) const LOOSE_ACI: u8 = 9;
/// "Will be refused" (`ecad.refuse-preview`, ACI 1, red).
pub(crate) const REFUSE_ACI: u8 = 1;

/// Glyph sizes in screen pixels (design tokens).
pub(crate) const CONNECTION_POINT_RING_PX: f64 = 8.0;
pub(crate) const RING_PX: f64 = 16.0;
const PREVIEW_TEXT_PX: f64 = 12.0;
/// Crossing dot of a cable slash on a member wire (`ecad.crossing-size`).
const CROSSING_PX: f32 = 5.0;

pub(crate) fn pt(v: Vec2) -> Pt {
    Pt::new(v.x, v.y)
}

pub(crate) fn v2(p: Pt) -> Vec2 {
    Vec2::new(p.x, p.y)
}

/// Whether a point may be used: finite and within [`MAX_COORD`].
pub(crate) fn in_range(p: Vec2) -> bool {
    p.is_finite() && p.x.abs() <= MAX_COORD && p.y.abs() <= MAX_COORD
}

/// Read the project owning the active drawing, with the page shown there. Refuses (with the
/// design's wording) off an ECAD page, on an unavailable page and when the project is busy.
pub(crate) fn read<R>(s: &Session, f: impl FnOnce(&Project, PageId) -> Result<R, String>) -> Result<R, String> {
    let owner = owner_of_active(s).map_err(|_| NOT_OWNED.to_string())?;
    if !model_space(s) {
        return Err(PAPER_SPACE.into());
    }
    let uid = s.state().map_err(|e| e.to_string())?.uid;
    let mut guard = try_lock(&owner)?;
    let p = guard.as_any_mut().downcast_mut::<Project>().ok_or(NOT_OWNED)?;
    let page = p.page_of(uid).ok_or(NOT_OWNED)?;
    if p.pages.get(&page).is_some_and(|v| v.unavailable) {
        return Err(crate::commands::UNAVAILABLE.into());
    }
    f(p, page)
}

/// The check every interactive form makes before its first prompt (design §3.4).
pub(crate) fn can_start(s: &Session) -> cadcraft_engine::Result<()> {
    read(s, |_, _| Ok(())).map_err(EngineError::Other)
}

/// Drawing units per screen pixel, when usable.
pub(crate) fn pixel(s: &Session) -> Option<f64> {
    let px = s.pixel_size();
    (px.is_finite() && px > 0.0).then_some(px)
}

/// Whether `p` is the cursor's own position (a click), rather than a typed point.
pub(crate) fn from_cursor(s: &Session, p: Vec2) -> bool {
    let tol = pixel(s).map_or(1e-9, |px| px * 1e-3);
    p.is_finite() && s.cursor.is_finite() && p.near(s.cursor, tol)
}

/// The pick box's radius in drawing units (as CADCraft's single-object prompts pick).
pub(crate) fn pick_radius(s: &Session) -> Option<f64> {
    Some(pixel(s)? * s.settings.pickbox.max(1.0) * 1.5)
}

/// The object under the pick box at `p` (as CADCraft's single-object prompts pick).
pub(crate) fn pick(s: &Session, p: Vec2) -> Option<Handle> {
    let r = pick_radius(s)?;
    if !in_range(p) {
        return None;
    }
    let h = s.pick_object(&s.space(), p, r).ok().flatten()?;
    // The sheet frame (brief 019) is not an object to pick. It is drawn behind everything else, so
    // it is only picked where nothing else is.
    let d = s.doc().ok()?;
    let frame = |h: Handle| d.entity(h).is_some_and(|e| e.common.layer == crate::pages::frame::LAYER);
    if !frame(h) {
        return Some(h);
    }
    (!frame(h)).then_some(h)
}

/// A terminal under the cursor.
#[derive(Debug, Clone, PartialEq)]
pub struct Capture {
    pub connection_point: ConnectionPointId,
    /// The terminal's position: the point used instead of the cursor.
    pub at: Pt,
    /// Its name as the preview shows it.
    pub name: String,
}

/// The terminal shown on `page` nearest to `cursor` within `aperture` drawing units, if any. At
/// equal distance the one the JSON form resolves at that point wins, so preview and transaction
/// agree.
pub fn capture(p: &Project, page: PageId, cursor: Vec2, aperture: f64) -> Option<Capture> {
    if !in_range(cursor) || !(aperture.is_finite() && aperture > 0.0) {
        return None;
    }
    let c = pt(cursor);
    let doc = p.ledger.document();
    let mut best: Option<(f64, Pt)> = None;
    for (_, _, at) in connection_point_positions(doc, page) {
        let d = at.dist(c);
        if d <= aperture && best.is_none_or(|(b, _)| d < b) {
            best = Some((d, at));
        }
    }
    let (_, at) = best?;
    let connection_point = connection_point_at(doc, page, at, p.tolerance)?;
    Some(Capture { connection_point, at, name: connection_point_name(doc, connection_point) })
}

/// The capture at the cursor, with the session's aperture (APERTURE pixels).
pub(crate) fn capture_at(s: &Session, cursor: Vec2) -> Option<Capture> {
    let aperture = pixel(s)? * s.settings.aperture.max(1.0);
    read(s, |p, page| Ok(capture(p, page, cursor, aperture))).ok().flatten()
}

/// The terminal a point resolves to: captured when it is the cursor's, else only a terminal
/// exactly there (the JSON form's own resolution; typed points are literal).
pub(crate) fn connection_point_for(s: &Session, p: Vec2) -> Option<(ConnectionPointId, Pt, String)> {
    if from_cursor(s, p) {
        return capture_at(s, p).map(|c| (c.connection_point, c.at, c.name));
    }
    read(s, |proj, page| {
        let doc = proj.ledger.document();
        Ok(connection_point_at(doc, page, pt(p), proj.tolerance).map(|t| (t, pt(p), connection_point_name(doc, t))))
    })
    .ok()
    .flatten()
}

/// The default name of the next terminal of component `c`: the smallest positive integer not
/// already used as a label among its terminals (owner answer Q2).
pub fn next_connection_point_name(doc: &Document, c: DeviceId) -> String {
    let used: BTreeSet<String> = doc.connection_points_of(c).filter_map(|(_, t)| t.label.clone()).collect();
    let mut n: u64 = 1;
    // At most one more than the labels in use: bounded by the component's terminals.
    while used.contains(&n.to_string()) && n <= used.len() as u64 {
        n += 1;
    }
    n.to_string()
}

/// Most characters in a typed terminal name.
pub const MAX_NAME: usize = 64;
/// Refusal of a typed name.
pub const BAD_NAME: &str = "Names are 1 to 64 printable characters.";

/// A typed terminal name, trimmed: 1 to [`MAX_NAME`] characters, none of them control characters.
pub fn valid_name(text: &str) -> Option<String> {
    let t = text.trim();
    let n = t.chars().count();
    ((1..=MAX_NAME).contains(&n) && !t.chars().any(char::is_control)).then(|| t.to_string())
}

/// A preview entity drawn in ACI colour `aci` (its own properties, not the current layer's).
pub(crate) fn preview_entity(s: &Session, kind: EntityKind, aci: u8) -> Entity {
    let mut common = s.current_common();
    common.color = CColor::Index(aci);
    Entity { handle: Handle(0), common, kind }
}

fn v3(p: Pt) -> Vec3 {
    Vec3::new(p.x, p.y, 0.0)
}

/// A circle of `diameter_px` screen pixels around `at`.
pub(crate) fn ring_kind(at: Pt, diameter_px: f64, px: f64) -> EntityKind {
    EntityKind::Circle(Circle { center: v3(at), radius: diameter_px * px / 2.0 })
}

pub(crate) fn line_kind(a: Pt, b: Pt) -> EntityKind {
    EntityKind::Line(Line { a: v3(a), b: v3(b) })
}

/// The "will attach" preview of a capture (design §4.2): a bold ring around the terminal and its
/// name beside it. The drafting font has no arrow, so the name reads `-> A1`.
pub(crate) fn capture_preview(s: &Session, c: &Capture, aci: u8) -> Vec<Entity> {
    let Some(px) = pixel(s) else { return Vec::new() };
    let mut out: Vec<Entity> = [RING_PX - 1.0, RING_PX, RING_PX + 1.0].into_iter().map(|d| preview_entity(s, ring_kind(c.at, d, px), aci)).collect();
    let at = Pt::new(c.at.x + 10.0 * px + RING_PX * px / 2.0, c.at.y + 10.0 * px);
    out.push(preview_text(s, at, px, format!("-> {}", c.name), aci));
    out
}

/// A preview text line (`ecad.preview-text`: 12 px high) at `at`, in ACI colour `aci`.
pub(crate) fn preview_text(s: &Session, at: Pt, px: f64, value: String, aci: u8) -> Entity {
    let text = Text {
        insert: v3(at),
        align_pt: None,
        height: PREVIEW_TEXT_PX * px,
        value,
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: Default::default(),
        valign: Default::default(),
    };
    preview_entity(s, EntityKind::Text(text), aci)
}

/// Where conductor routes on `page` start and end, once per conductor (its first route).
pub(crate) fn route_ends(doc: &Document, page: PageId) -> Vec<(RouteId, Pt, Pt)> {
    doc.routes_on(page).filter_map(|r| doc.routes.get(&r).and_then(|x| x.end_points()).map(|[a, b]| (r, a, b))).collect()
}

/// The marks shown over page drawing `uid` (design 008 §4.1): a ring on every terminal shown, a
/// dot on every attached wire end, a square on every loose one; (design 004 §7.2) a magenta dot
/// where a cable slash crosses one of its own members; and (design 010 §6.1, §6.4) a bold ring on
/// every cable reference point, magenta when linked and red for an orphan, with no end mark on a
/// wire end that leaves toward a reference. Display only.
pub fn overlay_marks(p: &Project, uid: u64) -> Vec<Mark> {
    // Brief 040: colours and sizes from `ecad-tokens` by meaning, on the sheet in use. An orphan
    // reference (error) and an end that looks connected (warning) are painted by the app in
    // shapes these marks lack (`draw::painted_marks`); none of them is an entity, so none prints.
    use ecad_drawing_style::marker::{CONNECTION_POINT_PX, LOOSE_END_PX, WIRE_END_ATTACHED_PX};
    let Some(page) = p.page_of(uid) else { return Vec::new() };
    if p.pages.get(&page).is_none_or(|v| v.unavailable) {
        return Vec::new();
    }
    let sheet = crate::cables::draw::sheet();
    let colour = |m: ecad_drawing_style::Meaning| crate::cables::draw::rgb(sheet.color(m));
    let doc = p.ledger.document();
    let positions = connection_point_positions(doc, page);
    let ring = colour(ecad_drawing_style::Meaning::ConnectionPoint);
    // A cable reference's named points are drawn by its marker, never as rings or dots (F4).
    let mut out: Vec<Mark> = positions
        .iter()
        .filter(|(t, _, _)| doc.named_points(*t).is_none())
        .map(|(_, _, at)| Mark { at: v2(*at), shape: MarkShape::Ring, size: CONNECTION_POINT_PX, stroke: 1.0, color: ring })
        .collect();
    // Rings at the reference ends (brief 010); loose-end squares are left out where a cable stops
    // at a reference or a wire leaves for another page (brief 009 `quiet`, with brief 010 §6.4
    // folded in).
    let markers = ecad_electrical::references::cables::markers_on_page(doc, page);
    let quiet = p.pages.get(&page).map(|v| v.map.quiet_ends()).unwrap_or_default();
    for (r, a, b) in route_ends(doc, page) {
        let Some(route) = doc.routes.get(&r) else { continue };
        for (at, landed) in [a, b].into_iter().zip(route.ends) {
            match landed {
                // An end on a connection point shown on this page: a dot on the point.
                RouteEnd::Point(t) => {
                    if let Some((_, _, q)) = positions.iter().find(|(x, _, _)| *x == t && doc.named_points(t).is_none()) {
                        let color = colour(ecad_drawing_style::Meaning::WireEndAttached);
                        out.push(Mark { at: v2(*q), shape: MarkShape::Dot, size: WIRE_END_ATTACHED_PX, stroke: 0.0, color });
                    }
                }
                // A T: its dot is drawn with the page (design 072 §4.4, `wires::tees_on_page`).
                RouteEnd::On { .. } => {}
                RouteEnd::Free if quiet.iter().any(|q| q.near(at, p.tolerance)) => {}
                RouteEnd::Free => {
                    let color = colour(ecad_drawing_style::Meaning::LooseEnd);
                    out.push(Mark { at: v2(at), shape: MarkShape::Square, size: LOOSE_END_PX, stroke: 1.0, color });
                }
            }
        }
    }
    let cable = colour(ecad_drawing_style::Meaning::Cable);
    let map = p.pages.get(&page).map(|v| &v.map);
    let shown = |i| map.is_some_and(|m| m.item(i).is_some());
    out.extend(ecad_electrical::cables::slash::member_crossing_points_where(doc, page, p.tolerance, shown).into_iter().map(|at| Mark {
        at: v2(at),
        shape: MarkShape::Dot,
        size: CROSSING_PX,
        stroke: 0.0,
        color: cable,
    }));
    out.extend(markers.into_iter().filter(|m| !m.orphan).map(|m| Mark {
        at: v2(m.at),
        shape: MarkShape::Ring,
        size: ecad_electrical::references::cables::RING_PX,
        stroke: ecad_electrical::references::cables::BOLD_PX,
        color: cable,
    }));
    // The Checks tab's go-to ring (design 022 §8), in the row's severity colour.
    if let Some((_, at, sev)) = p.goto_ring.filter(|(pg, _, _)| *pg == page) {
        let color = colour(match sev {
            ecad_electrical::checks::Severity::Error => ecad_drawing_style::Meaning::Error,
            ecad_electrical::checks::Severity::Warning => ecad_drawing_style::Meaning::Warning,
            ecad_electrical::checks::Severity::Info => ecad_drawing_style::Meaning::Ink2,
        });
        out.push(Mark { at: v2(at), shape: MarkShape::Ring, size: 24.0, stroke: 2.0, color });
    }
    out
}

fn tag(props: &ecad_model::Properties) -> Option<&str> {
    props.text(ecad_model::naming::keys::TAG)
}

/// What hovering drawing object `h` on page drawing `uid` shows (design §4.3): its name or ID and
/// its connections, or `None` for plain geometry and objects ECAD does not know. Read only.
pub fn hover_lines(p: &Project, uid: u64, h: Handle) -> Option<Vec<String>> {
    let page = p.page_of(uid)?;
    let view = p.pages.get(&page)?;
    let doc = p.ledger.document();
    let mut lines = match view.map.get(h)? {
        // A reference end, by its text, its point or 009's flag: brief 010's hover (one source).
        Mapped::RefText(r) => ecad_electrical::references::cables::hover_lines(doc, r)?,
        Mapped::Fan(_) if view.map.reference_of(h).is_some() => ecad_electrical::references::cables::hover_lines(doc, view.map.reference_of(h)?)?,
        // The sheet frame and title block (design 019 §4): where to change it.
        Mapped::Frame(_) => {
            vec![format!("Page {}: change it in the Properties panel", ecad_electrical::references::cables::page_number(doc, page))]
        }
        // A device's tag text or terminal label (brief 018): the device's lens.
        Mapped::Tag(r) => match doc.representations.get(&r)?.subject {
            Subject::Device(c) => with_device_references(doc, c, r, crate::devices::draw::hover_lines(doc, c, r)),
            _ => return None,
        },
        // A device cross-reference (brief 025): a table row's pole, the table's contacts, or the
        // contact's lens with its coil.
        Mapped::DeviceReference(r) => {
            // Brief 049 (design 049 §5.4): an arrow's reference text, bracket or caption.
            if let Subject::Aid(a) = doc.representations.get(&r)?.subject {
                ecad_electrical::continuations::arrow(doc, r)?;
                return Some(ecad_electrical::continuations::hover_lines(doc, a, r));
            }
            let Subject::Device(c) = doc.representations.get(&r)?.subject else { return None };
            let row = view.map.device_reference_part(h).and_then(|(_, i)| {
                let dev = ecad_electrical::references::devices::device(doc, c);
                let mut places = ecad_electrical::references::devices::Places::new(doc);
                let t = crate::references::draw::devices::table(doc, &mut places, &dev)?;
                t.parts.get(usize::try_from(i).ok()?)?.row
            });
            match row {
                Some(i) => ecad_electrical::references::devices::row_lines(doc, c, i, &crate::styles::draw::EngineMetrics)?,
                None if ecad_electrical::references::devices::device(doc, c).head == Some(r) => {
                    ecad_electrical::references::devices::head_lines(doc, c, r)
                }
                None => with_device_references(doc, c, r, crate::devices::draw::hover_lines(doc, c, r)),
            }
        }
        // A conductor label (brief 022): its wire's lens.
        Mapped::Label(w) => wire_lines(doc, Some(w), ecad_electrical::wires::route_of_wire_on(doc, w, page), page),
        // A T's dot (lane 072 D): the lens of the line that ends there.
        Mapped::Tee(r) => wire_lines(doc, ecad_electrical::wires::wire_of_route(doc, r), Some(r), page),
        // A link line (brief 030, design 030 §5.3).
        Mapped::JumperBar(c, n) => {
            vec![ecad_electrical::terminal_strips::jumper_bars::hover(doc, p.kept_reports().and_then(|o| o.strips.get(c)), c, n)]
        }
        Mapped::Item(i) => match doc.representation_of_item(i)? {
            (rid, r) if ecad_electrical::references::cables::is_reference(r) => ecad_electrical::references::cables::hover_lines(doc, rid)?,
            (rid, r) => match r.subject {
                // A device (a tagged component, brief 018): design 018 §6.4's lens.
                Subject::Device(c) if ecad_electrical::devices::tag_of(doc, c).is_some() => {
                    with_device_references(doc, c, rid, crate::devices::draw::hover_lines(doc, c, rid))
                }
                // Brief 049 (design 049 §5.4): a continuation arrow's other ends.
                Subject::Device(c) => component_lines(doc, c),
                Subject::Wire(w) => wire_lines(doc, Some(w), None, page),
                Subject::Route(x) => wire_lines(doc, ecad_electrical::wires::wire_of_route(doc, x), Some(x), page),
                // Brief 049 (design 049 §5.4): a continuation arrow's other ends (design 072 §6.2).
                Subject::Aid(a) if ecad_electrical::continuations::arrow(doc, rid).is_some() => {
                    ecad_electrical::continuations::hover_lines(doc, a, rid)
                }
                Subject::Aid(_) => Vec::new(),
                // A drawn slash: traditional, or a fan that cannot be drawn on this page.
                Subject::Cable(k) => {
                    let shown = match p.view_of(k) {
                        View::Traditional => "View: traditional",
                        View::Fan => match fan_on_page(doc, page, k, p.tolerance) {
                            FanOnPage::Failed => "View: fan (cannot be drawn on this page)",
                            FanOnPage::TooFew => "View: fan (needs two wires on this page)",
                            _ => "View: fan",
                        },
                    };
                    with_drawings(doc, k, with_view(cable_lines(doc, page, k), shown))
                }
            },
        },
        // A cable reference's named point (design 072 §4.4, F4) is the reference's: its lens.
        Mapped::Marker(rid, _) if doc.representations.get(&rid).is_some_and(ecad_electrical::references::cables::is_reference) => {
            ecad_electrical::references::cables::hover_lines(doc, rid)?
        }
        Mapped::Marker(_, t) => connection_point_lines(doc, t),
        Mapped::Fan(k) => {
            let fan = view.map.is_fan(k);
            let mut lines = with_view(cable_lines(doc, page, k), if fan { "View: fan" } else { "View: traditional" });
            if fan {
                let d = ecad_electrical::cables::layout::cable_on_page(doc, page, k, true, p.tolerance, &crate::cables::draw::font_width);
                lines.extend(
                    d.at_reference
                        .iter()
                        .map(|w| format!("{} is attached at the reference end.", ecad_electrical::cables::slash::wire_name(doc, *w))),
                );
            }
            // A tail, bend or label names its core second (design 009 §8).
            if let Some(w) = view.map.core_of(h) {
                let core = core_of(doc, k, w).unwrap_or_else(|| "none".into());
                lines.insert(1.min(lines.len()), format!("Core: {core} ({})", ecad_electrical::cables::slash::wire_name(doc, w)));
            }
            with_drawings(doc, k, lines)
        }
    };
    capped(&mut lines);
    Some(lines)
}

/// The terminal shown on page drawing `uid` within `aperture` drawing units of `cursor` (the
/// nearest, as a capture picks it), with what hovering it shows: drawn and symbol terminals
/// alike, connected or free; never a cable reference's named point. A terminal under the pointer wins over the objects around it (the
/// wire ending on it, the symbol it belongs to). Read only.
pub fn hover_connection_point(p: &Project, uid: u64, cursor: Vec2, aperture: f64) -> Option<(ConnectionPointId, Vec<String>)> {
    let page = p.page_of(uid)?;
    if p.pages.get(&page).is_none_or(|v| v.unavailable) {
        return None;
    }
    let c = capture(p, page, cursor, aperture)?;
    // A cable reference's named point (design 072 §4.4, F4) is the reference's: its own lens.
    if p.ledger.document().cable_point(c.connection_point).is_some() {
        return None;
    }
    let mut lines = connection_point_lines(p.ledger.document(), c.connection_point);
    capped(&mut lines);
    Some((c.connection_point, lines))
}

/// The cable reference end on page drawing `uid` under `cursor` (its point within `radius`
/// drawing units, or its text), with what hovering it shows (design 010 §8). It wins over the
/// objects around it: a continued end's middle wire starts exactly on its point. Read only.
pub fn hover_reference(p: &Project, uid: u64, cursor: Vec2, radius: f64) -> Option<(ecad_model::RepresentationId, Vec<String>)> {
    let page = p.page_of(uid)?;
    if p.pages.get(&page).is_none_or(|v| v.unavailable) || !in_range(cursor) {
        return None;
    }
    let doc = p.ledger.document();
    let r = crate::references::draw::cables::reference_at(doc, page, pt(cursor), radius)?;
    let mut lines = ecad_electrical::references::cables::hover_lines(doc, r)?;
    capped(&mut lines);
    Some((r, lines))
}

/// A cable's hover with the pages it is drawn on and its references, after the view line
/// (design 010 §8), when it has a reference.
fn with_drawings(doc: &Document, k: CableId, mut lines: Vec<String>) -> Vec<String> {
    let references: Vec<_> = ecad_electrical::references::cables::all(doc).into_iter().filter(|r| r.cable == k).collect();
    if references.is_empty() {
        return lines;
    }
    let members = doc.cables.get(&k).map(|c| c.cores.as_slice()).unwrap_or_default();
    let member_pages: std::collections::BTreeSet<PageId> = members.iter().flat_map(|m| ecad_electrical::cables::member_pages(doc, m)).collect();
    let mut pages: Vec<PageId> = Vec::new();
    for p in &doc.page_order {
        let drawn = doc.representations_of(Subject::Cable(k)).any(|(_, r)| r.page == *p) || member_pages.contains(p);
        if drawn {
            pages.push(*p);
        }
    }
    let at = lines.len().min(2);
    let words: Vec<String> = pages.iter().map(|p| ecad_electrical::references::cables::page_number_words(doc, *p)).collect();
    let shown = match references.as_slice() {
        [one] => format!("References: {} (orphan)", one.text),
        many => format!("References: {}", many.iter().map(|r| r.locator()).collect::<Vec<_>>().join(" <-> ")),
    };
    lines.insert(at, shown);
    lines.insert(at, format!("Drawn on: {}", words.join(", ")));
    lines
}

/// A cable's hover with its view after the `id …` line (design 005 §4.2).
fn with_view(mut lines: Vec<String>, view: &str) -> Vec<String> {
    lines.insert(lines.len().min(2), view.to_string());
    lines
}

fn capped(lines: &mut Vec<String>) {
    if lines.len() > MAX_HOVER_LINES {
        let more = lines.len() - (MAX_HOVER_LINES - 1);
        lines.truncate(MAX_HOVER_LINES - 1);
        lines.push(format!("… and {more} more"));
    }
}

fn component_lines(doc: &Document, c: DeviceId) -> Vec<String> {
    let component = doc.devices.get(&c);
    let mut lines = vec![format!("Device {}", component.and_then(|x| x.tag()).unwrap_or("(no tag)")), format!("id {}", c.hex())];
    let connection_points: Vec<(ConnectionPointId, String)> =
        doc.connection_points_of(c).map(|(t, x)| (t, x.label.clone().unwrap_or_else(|| "(no label)".into()))).collect();
    lines.push(format!("Connection points: {}", connection_points.len()));
    for (t, label) in connection_points {
        let k = doc.ends_at(t).len();
        lines.push(format!("  {label} — {k} wire end{}", if k == 1 { "" } else { "s" }));
    }
    lines
}

fn connection_point_lines(doc: &Document, t: ConnectionPointId) -> Vec<String> {
    let term = doc.connection_points.get(&t);
    let of = term
        .map(|x| {
            let c = x.device().and_then(|c| doc.devices.get(&c));
            c.and_then(|c| c.tag()).map_or_else(|| x.owner.subject_hex(), str::to_string)
        })
        .unwrap_or_default();
    let mut lines = vec![
        // Brief 044 (tester D3): a connector pin by its half (`-X10.S:3`).
        format!(
            "Connection point {}",
            ecad_electrical::connectors::pin_name(doc, t).or_else(|| term.and_then(|x| x.label.clone())).unwrap_or_else(|| "(no label)".into())
        ),
        format!("of component {of}"),
        format!("id {}", t.hex()),
    ];
    let ends = doc.ends_at(t);
    if ends.is_empty() {
        lines.push("Wires: none".into());
    } else {
        lines.push("Wires:".into());
        lines.extend(ends.into_iter().map(|(w, end)| format!("  wire {}… end {end:?}", short(&w.hex()))));
    }
    lines
}

/// The core label of member `w` of cable `k`, when it has one that is not blank, as drawn.
fn core_of(doc: &Document, k: CableId, w: WireId) -> Option<String> {
    ecad_electrical::cables::member_core(doc.cables.get(&k)?.cores.iter().find(|m| m.wire() == Some(w))?)
}

/// A cable slash (design 004 §7.3, design 009 §8): its tag, ID, designation, type and reference
/// ends on `page`, and its members in stored order with their cores.
fn cable_lines(doc: &Document, page: PageId, k: CableId) -> Vec<String> {
    let cable = doc.cables.get(&k);
    let mut lines = vec![format!("Cable {}", cable.and_then(|x| tag(&x.properties)).unwrap_or("(no tag)")), format!("id {}", k.hex())];
    for (key, label) in [("designation", "Designation"), ("type", "Type")] {
        if let Some(v) = cable.and_then(|x| x.properties.get(key)) {
            lines.push(format!("{label}: {}", ecad_electrical::cables::words::shorten(v, 64)));
        }
    }
    let references: Vec<String> = doc
        .representations
        .iter()
        .filter(|(_, r)| r.page == page && r.subject == Subject::Cable(k) && r.role == ecad_model::Role::Reference)
        .map(|(id, _)| format!("\"{}\"", ecad_electrical::cables::layout::reference_text(doc, *id)))
        .collect();
    if !references.is_empty() {
        lines.push(format!("References here: {}", references.join(", ")));
    }
    // Brief 028 (design 028 §7.1): the core count.
    if let Some(line) = ecad_electrical::terminal_strips::cable_hover(doc, k) {
        lines.insert(1, line);
    }
    let members = cable.map(|x| x.cores.as_slice()).unwrap_or_default();
    if members.is_empty() {
        lines.push("Wires: none".into());
    } else {
        // The stored member order, nothing more (owner answer Q2).
        lines.push(format!("Wires: {}", members.len()));
        lines.extend(members.iter().map(|m| match ecad_electrical::cables::member_core(m) {
            Some(c) => format!("  {}  core {c}", ecad_electrical::cables::slash::position_name(doc, m.position)),
            None => format!("  {}", ecad_electrical::cables::slash::position_name(doc, m.position)),
        }));
    }
    lines
}

/// A wire's lens: wire `w` (the one a clicked line stands for, when it has one) and the open ends
/// of route `route` (the line clicked, or the wire's line on `page`).
fn wire_lines(doc: &Document, w: Option<WireId>, route: Option<RouteId>, page: PageId) -> Vec<String> {
    let conductor = w.and_then(|w| doc.wires.get(&w));
    let mut lines = match w {
        Some(w) => vec![format!("Wire {}", conductor.and_then(|x| x.number_text()).unwrap_or("(no number)")), format!("id {}", w.hex())],
        None => vec!["Wire line (no connection points yet)".to_string()],
    };
    if let Some((w, k)) = w.and_then(|w| doc.cables_of(w).first().map(|k| (w, *k))) {
        let name = match doc.cables.get(&k).and_then(|x| tag(&x.properties)) {
            Some(t) => t.to_string(),
            None => format!("{}…", short(&k.hex())),
        };
        match core_of(doc, k, w) {
            Some(c) => lines.push(format!("In cable {name}, core {c}")),
            None => lines.push(format!("In cable {name}")),
        }
    }
    if let Some(c) = conductor {
        for end in End::BOTH {
            let t = c.end(end);
            lines.push(match doc.connection_points.get(&t) {
                // A continuation (brief 022) is named by what it is.
                Some(x) if x.owner.aid().is_some() => {
                    let words = ecad_electrical::wires::point_words(doc, t);
                    match words.strip_prefix('\u{2192}') {
                        Some(name) => format!("End {end:?}: continuation {name}"),
                        None => format!("End {end:?}: {words}"),
                    }
                }
                Some(x) => {
                    let of = x
                        .device()
                        .and_then(|c| doc.devices.get(&c))
                        .and_then(|c| c.tag())
                        .map_or_else(|| short(&x.owner.subject_hex()).to_string(), str::to_string);
                    format!("End {end:?}: connection point {} of {of}", connection_point_name(doc, t))
                }
                None => format!("End {end:?}: not connected"),
            });
        }
    }
    // Brief 032 §2.6: an open end of the line lying on another line says so, and how to join them.
    if let Some(r) = route
        && let Some(line) = doc.routes.get(&r)
    {
        for ((at, landed), end) in line.end_points().into_iter().flatten().zip(line.ends).zip(End::BOTH) {
            match landed {
                // A T (design 072 §4.4) is joined, not open: it names the line it lands on.
                RouteEnd::On { route: on, .. } => {
                    let joined = match ecad_electrical::wires::wire_of_route(doc, on) {
                        Some(w) => format!("wire {}", ecad_model::connectivity::wiring::wire_number(doc, w).unwrap_or_else(|| "(no number)".into())),
                        None => "a line with no wire yet".into(),
                    };
                    lines.push(format!("End {end:?} joins {joined} (a T)."));
                }
                RouteEnd::Free => {
                    if let Some(on) = ecad_electrical::wires::route_at(doc, page, at, ecad_electrical::wires::ON_WIRE, Some(r)) {
                        let number = ecad_electrical::wires::wire_of_route(doc, on.route)
                            .and_then(|w| ecad_model::connectivity::wiring::wire_number(doc, w))
                            .unwrap_or_else(|| "?".into());
                        lines.push(format!("Open end: looks connected to {number}; Join Wires joins them."));
                    }
                }
                RouteEnd::Point(_) => {}
            }
        }
    }
    let Some(w) = w else { return lines };
    // Brief 010: a wire continued on other pages says where.
    let others: Vec<String> = ecad_electrical::references::cables::route_pages(doc, w)
        .into_iter()
        .filter(|p| *p != page)
        .map(|p| ecad_electrical::references::cables::page_number_words(doc, p))
        .collect();
    if !others.is_empty() {
        lines.push(format!("Also drawn on {}", others.join(", ")));
    }
    // Brief 022: the wire's number, potential, label and net (the Properties panel's facts until
    // 019's panel is merged).
    lines.extend(ecad_electrical::wires::hover_lines(doc, w));
    lines
}

/// A device's lens (design 018 §6.4) with its cross-reference lines (design 025 §5.1): the coil's
/// place on a contact, the table's contacts on a coil, before the lens's last (`id`) line.
fn with_device_references(doc: &ecad_model::Document, c: DeviceId, rep: ecad_model::RepresentationId, mut lines: Vec<String>) -> Vec<String> {
    let mut extra = ecad_electrical::references::devices::contact_lines(doc, c, rep);
    extra.extend(ecad_electrical::references::devices::head_lines(doc, c, rep));
    let at = lines.len().saturating_sub(1);
    for (i, l) in extra.into_iter().enumerate() {
        lines.insert(at + i, l);
    }
    lines
}

/// The drawn component `h` is an outline item of, with the item's primitive, or the reason it is
/// not one (design §5's wording).
pub(crate) fn drawn_component(p: &Project, page: PageId, h: Handle) -> Result<(DeviceId, Primitive), String> {
    let map = &p.pages.get(&page).ok_or("page view not found")?.map;
    let doc = p.ledger.document();
    let item = match map.get(h) {
        Some(Mapped::Item(i)) => i,
        Some(Mapped::Marker(..) | Mapped::Tag(_) | Mapped::Label(_) | Mapped::DeviceReference(_)) => return Err(NOT_OUTLINE.into()),
        Some(Mapped::Fan(k)) => return Err(crate::cables::views::refusal(map, k).into()),
        Some(Mapped::Frame(_)) => return Err(crate::pages::frame::DERIVED.into()),
        Some(Mapped::JumperBar(..)) => return Err(ecad_electrical::terminal_strips::jumper_bars::JUMPER_BAR_DERIVED.into()),
        Some(Mapped::Tee(_)) => return Err(ecad_electrical::wires::TEE_DERIVED.into()),
        Some(Mapped::RefText(_)) | None => return Err(NOT_COMPONENT.into()),
    };
    let Some((_, rep)) = doc.representation_of_item(item) else { return Err(NOT_COMPONENT.into()) };
    let Subject::Device(c) = rep.subject else { return Err(NOT_OUTLINE.into()) };
    if rep.form != Form::Drawn {
        return Err("connection points of placed symbols come from their definition".into());
    }
    match doc.items.get(&item).map(|i| &i.kind) {
        Some(ItemKind::Primitive(prim)) => Ok((c, prim.clone())),
        _ => Err(NOT_OUTLINE.into()),
    }
}

pub(crate) const NO_OUTLINE: &str = "No component outline there. Click on a component's outline.";
pub(crate) const NOT_COMPONENT: &str = "That object is not part of a component. Convert it to a component first.";
pub(crate) const NOT_OUTLINE: &str = "That is not a component outline.";
