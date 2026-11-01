//! Click-based forms of the ECAD commands (brief 008), and what the drawing shows about ECAD
//! objects: terminal and wire-end marks, the capture of a terminal under the cursor, and the
//! hover description.
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

mod forms;

use std::collections::BTreeSet;

use cadcraft_engine::doc::color::{Color as CColor, Rgb};
use cadcraft_engine::doc::{Circle, Entity, EntityKind, Handle, Line, Text};
use cadcraft_engine::ext::{Mark, MarkShape};
use cadcraft_engine::geom::{Vec2, Vec3};
use cadcraft_engine::{EngineError, Session};
use ecad_model::derive::terminal_positions;
use ecad_model::{ComponentId, ConductorId, Document, End, Form, ItemKind, PageId, Primitive, Pt, Subject, TerminalId};

use crate::access::{owner_of_active, try_lock};
use crate::project::{Mapped, Project};
use crate::translate::terminal_at;

pub(crate) use forms::{addterminal, connect, convert, disconnect, place, wire};

/// Refusal on a drawing that is not an ECAD page (design §3.4).
pub const NOT_OWNED: &str = "this page is not owned by an ECAD project";
/// Typed coordinates beyond this (or not finite) are refused (design §9).
pub const MAX_COORD: f64 = 1e12;
/// Refusal of a point that is not finite or out of range.
pub const OUT_OF_RANGE: &str = "Point out of range.";

/// Canvas colours (design tokens, ACI on the dark canvas).
pub(crate) const TERMINAL_ACI: u8 = 4;
pub(crate) const ATTACHED_ACI: u8 = 3;
/// The owner chose a neutral colour for loose ends, not red (brief 008 answer Q3): light grey.
pub(crate) const LOOSE_ACI: u8 = 9;
const TERMINAL_RGB: Rgb = Rgb(0x00, 0xff, 0xff);
const ATTACHED_RGB: Rgb = Rgb(0x00, 0xff, 0x00);
const LOOSE_RGB: Rgb = Rgb(0xc0, 0xc0, 0xc0);

/// Glyph sizes in screen pixels (design tokens).
pub(crate) const TERMINAL_PX: f64 = 8.0;
const DOT_PX: f32 = 4.0;
const LOOSE_PX: f32 = 6.0;
pub(crate) const RING_PX: f64 = 16.0;
const PREVIEW_TEXT_PX: f64 = 12.0;

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

/// The object under the pick box at `p` (as CADCraft's single-object prompts pick).
pub(crate) fn pick(s: &Session, p: Vec2) -> Option<Handle> {
    let px = pixel(s)?;
    if !in_range(p) {
        return None;
    }
    s.pick_object(&s.space(), p, px * s.settings.pickbox.max(1.0) * 1.5).ok().flatten()
}

/// A terminal under the cursor.
#[derive(Debug, Clone, PartialEq)]
pub struct Capture {
    pub terminal: TerminalId,
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
    for (_, _, at) in terminal_positions(doc, page) {
        let d = at.dist(c);
        if d <= aperture && best.is_none_or(|(b, _)| d < b) {
            best = Some((d, at));
        }
    }
    let (_, at) = best?;
    let terminal = terminal_at(doc, page, at, p.tolerance)?;
    Some(Capture { terminal, at, name: terminal_name(doc, terminal) })
}

/// The capture at the cursor, with the session's aperture (APERTURE pixels).
pub(crate) fn capture_at(s: &Session, cursor: Vec2) -> Option<Capture> {
    let aperture = pixel(s)? * s.settings.aperture.max(1.0);
    read(s, |p, page| Ok(capture(p, page, cursor, aperture))).ok().flatten()
}

/// The terminal a point resolves to: captured when it is the cursor's, else only a terminal
/// exactly there (the JSON form's own resolution; typed points are literal).
pub(crate) fn terminal_for(s: &Session, p: Vec2) -> Option<(TerminalId, Pt, String)> {
    if from_cursor(s, p) {
        return capture_at(s, p).map(|c| (c.terminal, c.at, c.name));
    }
    read(s, |proj, page| {
        let doc = proj.ledger.document();
        Ok(terminal_at(doc, page, pt(p), proj.tolerance).map(|t| (t, pt(p), terminal_name(doc, t))))
    })
    .ok()
    .flatten()
}

fn short(hex: &str) -> &str {
    hex.get(..8).unwrap_or(hex)
}

/// A terminal's name: its label, or `terminal <first 8 hex digits>`.
pub fn terminal_name(doc: &Document, t: TerminalId) -> String {
    match doc.terminals.get(&t).and_then(|x| x.label.clone()) {
        Some(l) => l,
        None => format!("terminal {}", short(&t.hex())),
    }
}

/// The default name of the next terminal of component `c`: the smallest positive integer not
/// already used as a label among its terminals (owner answer Q2).
pub fn next_terminal_name(doc: &Document, c: ComponentId) -> String {
    let used: BTreeSet<String> = doc.terminals_of(c).filter_map(|(_, t)| t.label.clone()).collect();
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

/// The point on `prim` nearest to `p` (its outline), or `None` for kinds without one.
pub fn nearest_on(prim: &Primitive, p: Pt) -> Option<Pt> {
    let seg = |a: Pt, b: Pt| {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len2 = dx * dx + dy * dy;
        if !(len2.is_finite() && len2 > 0.0) {
            return a;
        }
        let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
        Pt::new(a.x + t * dx, a.y + t * dy)
    };
    let best =
        |cands: Vec<Pt>| cands.into_iter().filter(|q| q.is_finite()).min_by(|a, b| a.dist(p).total_cmp(&b.dist(p)));
    match prim {
        Primitive::Line { a, b } => best(vec![seg(*a, *b)]),
        Primitive::Polyline { vertices, closed } => {
            let mut cands: Vec<Pt> = vertices.windows(2).filter_map(|w| Some(seg(*w.first()?, *w.get(1)?))).collect();
            if *closed && let (Some(a), Some(b)) = (vertices.last(), vertices.first()) {
                cands.push(seg(*a, *b));
            }
            if vertices.len() == 1 {
                cands.extend(vertices.first().copied());
            }
            best(cands)
        }
        Primitive::Circle { center, radius } => on_circle(*center, *radius, p),
        Primitive::Arc { center, radius, start, end } => {
            let on = on_circle(*center, *radius, p)?;
            let angle = (on.y - center.y).atan2(on.x - center.x);
            let tau = std::f64::consts::TAU;
            let sweep = (end - start).rem_euclid(tau);
            if (angle - start).rem_euclid(tau) <= sweep {
                return Some(on);
            }
            let at = |a: f64| Pt::new(center.x + radius * a.cos(), center.y + radius * a.sin());
            best(vec![at(*start), at(*end)])
        }
        Primitive::Text { .. } | Primitive::Point { .. } => None,
    }
}

fn on_circle(center: Pt, radius: f64, p: Pt) -> Option<Pt> {
    let d = p.dist(center);
    if !(d.is_finite() && d > 0.0 && radius.is_finite()) {
        return None;
    }
    Some(Pt::new(center.x + (p.x - center.x) * radius / d, center.y + (p.y - center.y) * radius / d))
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
    let mut out: Vec<Entity> = [RING_PX - 1.0, RING_PX, RING_PX + 1.0]
        .into_iter()
        .map(|d| preview_entity(s, ring_kind(c.at, d, px), aci))
        .collect();
    let at = Pt::new(c.at.x + 10.0 * px + RING_PX * px / 2.0, c.at.y + 10.0 * px);
    let text = Text {
        insert: v3(at),
        align_pt: None,
        height: PREVIEW_TEXT_PX * px,
        value: format!("-> {}", c.name),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: Default::default(),
        valign: Default::default(),
    };
    out.push(preview_entity(s, EntityKind::Text(text), aci));
    out
}

/// Where conductor routes on `page` start and end, once per conductor (its first route).
fn route_ends(doc: &Document, page: PageId) -> Vec<(ConductorId, Pt, Pt)> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for r in doc.representations.values().filter(|r| r.page == page) {
        let Subject::Conductor(c) = r.subject else { continue };
        if seen.contains(&c) {
            continue;
        }
        let Some(ItemKind::Primitive(prim)) = r.items.first().and_then(|i| doc.items.get(i)).map(|i| &i.kind) else {
            continue;
        };
        if let Some((a, b)) = prim.path_ends() {
            seen.insert(c);
            out.push((c, a, b));
        }
    }
    out
}

/// The marks shown over page drawing `uid` (design §4.1): a ring on every terminal shown, a dot
/// on every attached wire end, a square on every loose one. Display only.
pub fn overlay_marks(p: &Project, uid: u64) -> Vec<Mark> {
    let Some(page) = p.page_of(uid) else { return Vec::new() };
    if p.pages.get(&page).is_none_or(|v| v.unavailable) {
        return Vec::new();
    }
    let doc = p.ledger.document();
    let positions = terminal_positions(doc, page);
    let mut out: Vec<Mark> = positions
        .iter()
        .map(|(_, _, at)| Mark {
            at: v2(*at),
            shape: MarkShape::Ring,
            size: TERMINAL_PX as f32,
            stroke: 1.0,
            color: TERMINAL_RGB,
        })
        .collect();
    for (c, a, b) in route_ends(doc, page) {
        let Some(conductor) = doc.conductors.get(&c) else { continue };
        for (end, at) in [(End::A, a), (End::B, b)] {
            out.push(match conductor.end(end) {
                Some(t) => {
                    let at = positions.iter().find(|(x, _, _)| *x == t).map_or(at, |(_, _, q)| *q);
                    Mark { at: v2(at), shape: MarkShape::Dot, size: DOT_PX, stroke: 0.0, color: ATTACHED_RGB }
                }
                None => Mark { at: v2(at), shape: MarkShape::Square, size: LOOSE_PX, stroke: 1.0, color: LOOSE_RGB },
            });
        }
    }
    out
}

fn tag(props: &ecad_model::Properties) -> Option<&str> {
    props.get("tag").map(String::as_str)
}

/// Most lines a hover description shows.
pub const MAX_HOVER_LINES: usize = 12;

/// What hovering drawing object `h` on page drawing `uid` shows (design §4.3): its name or ID and
/// its connections, or `None` for plain geometry and objects ECAD does not know. Read only.
pub fn hover_lines(p: &Project, uid: u64, h: Handle) -> Option<Vec<String>> {
    let page = p.page_of(uid)?;
    let view = p.pages.get(&page)?;
    let doc = p.ledger.document();
    let mut lines = match view.map.get(h)? {
        Mapped::Item(i) => match doc.representation_of_item(i)?.1.subject {
            Subject::Component(c) => component_lines(doc, c),
            Subject::Conductor(w) => wire_lines(doc, w),
            Subject::Cable(_) => return None,
        },
        Mapped::Marker(_, t) => terminal_lines(doc, t),
    };
    if lines.len() > MAX_HOVER_LINES {
        let more = lines.len() - (MAX_HOVER_LINES - 1);
        lines.truncate(MAX_HOVER_LINES - 1);
        lines.push(format!("… and {more} more"));
    }
    Some(lines)
}

fn component_lines(doc: &Document, c: ComponentId) -> Vec<String> {
    let component = doc.components.get(&c);
    let mut lines = vec![
        format!("Component {}", component.and_then(|x| tag(&x.properties)).unwrap_or("(no tag)")),
        format!("id {}", c.hex()),
    ];
    let terminals: Vec<(TerminalId, String)> =
        doc.terminals_of(c).map(|(t, x)| (t, x.label.clone().unwrap_or_else(|| "(no label)".into()))).collect();
    lines.push(format!("Terminals: {}", terminals.len()));
    for (t, label) in terminals {
        let k = doc.ends_at(t).len();
        lines.push(format!("  {label} — {k} wire end{}", if k == 1 { "" } else { "s" }));
    }
    lines
}

fn terminal_lines(doc: &Document, t: TerminalId) -> Vec<String> {
    let term = doc.terminals.get(&t);
    let of = term
        .map(|x| {
            let c = doc.components.get(&x.component);
            c.and_then(|c| tag(&c.properties)).map_or_else(|| x.component.hex(), str::to_string)
        })
        .unwrap_or_default();
    let mut lines = vec![
        format!("Terminal {}", term.and_then(|x| x.label.clone()).unwrap_or_else(|| "(no label)".into())),
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

fn wire_lines(doc: &Document, w: ConductorId) -> Vec<String> {
    let conductor = doc.conductors.get(&w);
    let mut lines = vec![
        format!("Wire {}", conductor.and_then(|x| tag(&x.properties)).unwrap_or("(no tag)")),
        format!("id {}", w.hex()),
    ];
    for end in End::BOTH {
        lines.push(match conductor.and_then(|x| x.end(end)) {
            Some(t) => {
                let of = doc
                    .terminals
                    .get(&t)
                    .map(|x| {
                        let c = doc.components.get(&x.component);
                        c.and_then(|c| tag(&c.properties))
                            .map_or_else(|| short(&x.component.hex()).to_string(), str::to_string)
                    })
                    .unwrap_or_default();
                format!("End {end:?}: terminal {} of {of}", terminal_name(doc, t))
            }
            None => format!("End {end:?}: not connected"),
        });
    }
    lines
}

/// The drawn component `h` is an outline item of, with the item's primitive, or the reason it is
/// not one (design §5's wording).
pub(crate) fn drawn_component(p: &Project, page: PageId, h: Handle) -> Result<(ComponentId, Primitive), String> {
    let map = &p.pages.get(&page).ok_or("page view not found")?.map;
    let doc = p.ledger.document();
    let item = match map.get(h) {
        Some(Mapped::Item(i)) => i,
        Some(Mapped::Marker(..)) => return Err(NOT_OUTLINE.into()),
        None => return Err(NOT_COMPONENT.into()),
    };
    let Some((_, rep)) = doc.representation_of_item(item) else { return Err(NOT_COMPONENT.into()) };
    let Subject::Component(c) = rep.subject else { return Err(NOT_OUTLINE.into()) };
    if rep.form != Form::Drawn {
        return Err("terminals of placed symbols come from their definition".into());
    }
    match doc.items.get(&item).map(|i| &i.kind) {
        Some(ItemKind::Primitive(prim)) => Ok((c, prim.clone())),
        _ => Err(NOT_OUTLINE.into()),
    }
}

pub(crate) const NO_OUTLINE: &str = "No component outline there. Click on a component's outline.";
pub(crate) const NOT_COMPONENT: &str = "That object is not part of a component. Convert it to a component first.";
pub(crate) const NOT_OUTLINE: &str = "That is not a component outline.";
