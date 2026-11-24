//! Reading the project behind the active drawing, and picking on it: the object under the pick
//! box, the connection point captured under the cursor (within the APERTURE) and the drawn
//! component an outline belongs to.
//!
//! The project is read with `try_lock` for prompts and previews, and released before any commit:
//! a busy project gives a plain prompt and no preview, never a deadlock. Typed points are taken
//! literally (the JSON form's own end resolution still applies to them).

use cadcraft_engine::doc::Handle;
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{EngineError, Session};
use ecad_electrical::commands::input::MAX_COORD;
use ecad_electrical::terminal_strips::terminal_blocks::connection_point_name;
use ecad_model::connectivity::derive::connection_point_positions;
use ecad_model::{ConnectionPointId, DeviceId, Form, ItemKind, PageId, Primitive, Pt, Subject};

use crate::access::{owner_of_active, try_lock};
use crate::project::{Mapped, Project};
use crate::translate::connection_point_at;

/// Refusal on a drawing that is not an ECAD page.
pub const NOT_OWNED: &str = "this page is not owned by an ECAD project";
/// Refusal while a paper-space layout is shown: ECAD objects live in the page's model space.
pub(crate) const PAPER_SPACE: &str = "Switch to the Model tab to draw ECAD objects.";

/// Whether the Model tab is shown: not a paper-space layout, nor a viewport of one (where
/// `Session::space` reports model space but the canvas shows the sheet).
pub(crate) fn model_space(s: &Session) -> bool {
    s.layout_space() == cadcraft_engine::doc::Space::Model
}

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
/// the usual wording) off an ECAD page, on an unavailable page and when the project is busy.
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

/// The check every interactive form makes before its first prompt.
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
    // The sheet frame is not an object to pick. It is drawn behind everything else, so
    // it is only picked where nothing else is.
    let d = s.doc().ok()?;
    let frame = |h: Handle| d.entity(h).is_some_and(|e| e.common.layer == ecad_electrical::pages::frames::LAYER);
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

/// The drawn component `h` is an outline item of, with the item's primitive, or the reason it is
/// not one.
pub(crate) fn drawn_component(p: &Project, page: PageId, h: Handle) -> Result<(DeviceId, Primitive), String> {
    let map = &p.pages.get(&page).ok_or("page view not found")?.map;
    let doc = p.ledger.document();
    let item = match map.get(h) {
        Some(Mapped::Item(i)) => i,
        Some(Mapped::Marker(..) | Mapped::Tag(_) | Mapped::Label(_) | Mapped::DeviceReference(_)) => return Err(NOT_OUTLINE.into()),
        Some(Mapped::Fan(k)) => return Err(crate::cables::views::refusal(map, k).into()),
        Some(Mapped::Frame(_)) => return Err(ecad_electrical::pages::frames::DERIVED.into()),
        Some(Mapped::JumperBar(..)) => return Err(ecad_electrical::terminal_strips::jumper_bars::JUMPER_BAR_DERIVED.into()),
        Some(Mapped::Tee(_)) => return Err(ecad_electrical::wires::TEE_DERIVED.into()),
        Some(Mapped::RefText(_)) | None => return Err(NOT_COMPONENT.into()),
    };
    let Some((_, rep)) = doc.representation_of_item(item) else { return Err(NOT_COMPONENT.into()) };
    let Subject::Device(c) = rep.subject else { return Err(NOT_OUTLINE.into()) };
    if rep.form != Form::Drawn {
        return Err("connection points of placed symbols come from their definition".into());
    }
    match doc.items().get(&item).map(|i| &i.kind) {
        Some(ItemKind::Primitive(prim)) => Ok((c, prim.clone())),
        _ => Err(NOT_OUTLINE.into()),
    }
}

pub(crate) const NO_OUTLINE: &str = "No component outline there. Click on a component's outline.";
pub(crate) const NOT_COMPONENT: &str = "That object is not part of a component. Convert it to a component first.";
pub(crate) const NOT_OUTLINE: &str = "That is not a component outline.";
