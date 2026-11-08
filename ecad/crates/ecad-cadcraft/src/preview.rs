//! The read-only page preview (brief 016, design 016 §5): another page drawn from the model into
//! an image, for checking a cable reference's other end without leaving the active page.
//!
//! Drawing a preview has two parts, so a caller can hold the project's lock only for the first:
//! [`preview_drawing`] projects the page from the document (under the lock, about a page's
//! projection), and [`rasterise`] builds CADCraft's display list and rasterises it with
//! `cadcraft-render` (with the project released). Nothing here changes the project, a page
//! window, a selection or any history.
//!
//! Excluded: terminal and wire-end marks (the trial's overlay), grips, selection and any editing.

use std::collections::BTreeSet;

use cadcraft_engine::doc::{Drawing, Space};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::render::{self as pipeline, raster};
use ecad_model::{PageId, Pt, RepresentationId, Subject};

use crate::project::{HandleMap, Project, base_drawing, project_page_with};

/// The width of drawing a preview on a reference shows, in millimetres (`ecad.preview-span`,
/// design 016 §9: four columns of 40 mm).
pub const PREVIEW_SPAN_MM: f64 = 160.0;

/// Where a preview looks: the drawing point at the centre of the image and the drawing height
/// the image shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewView {
    pub center: Pt,
    pub height: f64,
}

impl PreviewView {
    /// The view of `drawing`'s extents in an image `width` × `height` pixels, with 5 % to spare
    /// (CADCraft's zoom extents); an empty page shows 0..297 × 0..210.
    pub fn extents(drawing: &Drawing, width: u32, height: u32) -> Self {
        let b = drawing.extents(&Space::Model);
        let (lo, hi) = if b.is_empty() { (Vec2::new(0.0, 0.0), Vec2::new(297.0, 210.0)) } else { (b.min, b.max) };
        let aspect = f64::from(width.max(1)) / f64::from(height.max(1));
        let h = (hi.y - lo.y).max((hi.x - lo.x) / aspect).max(1e-6) * 1.05;
        PreviewView { center: Pt::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0), height: h }
    }

    /// A view centred on `at`, `span` drawing units wide in an image of `width` × `height`.
    pub fn around(at: Pt, span: f64, width: u32, height: u32) -> Self {
        let aspect = f64::from(width.max(1)) / f64::from(height.max(1));
        PreviewView { center: at, height: (span / aspect).max(1e-6) }
    }

    /// Pixels per drawing unit in an image `height` pixels high.
    pub fn scale(&self, height: u32) -> f64 {
        f64::from(height.max(1)) / self.height.max(1e-9)
    }

    /// Whether the view can be drawn: finite, with a height that is positive and not absurd.
    pub fn usable(&self) -> bool {
        self.center.is_finite() && self.height.is_finite() && self.height > 1e-9 && self.height < 1e12
    }
}

/// One rasterised preview: premultiplied RGBA, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The other end of the cable reference `reference`: its page and point (design 016 §5.3).
///
/// # Errors
/// In words, as Preview Reference says them: not a reference, an orphan, or an end that cannot
/// be located.
pub fn other_end(p: &Project, reference: RepresentationId) -> Result<(PageId, Pt), String> {
    let doc = p.ledger.document();
    let cable = match doc.representations.get(&reference) {
        Some(r) if crate::references::is_reference(r) => match r.subject {
            Subject::Cable(k) => k,
            _ => return Err(NOT_A_REFERENCE.into()),
        },
        _ => return Err(NOT_A_REFERENCE.into()),
    };
    let refs = ecad_model::derive::cable_references_of(doc, &BTreeSet::from([cable]));
    let here = refs.iter().find(|r| r.reference == reference).ok_or(NOT_A_REFERENCE)?;
    let other = here.partners.first().copied().ok_or(ORPHAN)?;
    let there = refs.iter().find(|r| r.reference == other).ok_or(ORPHAN)?;
    let at = there.at.filter(|a| a.is_finite()).ok_or("The other end of this reference cannot be located.")?;
    Ok((there.page, at))
}

/// Said when Preview Reference has no cable reference to work on.
pub const NOT_A_REFERENCE: &str = "Select a cable reference (its point or text), then click Preview Reference.";
/// Said when the selected reference is an orphan.
pub const ORPHAN: &str = "This reference has no other end (orphan).";

/// Project `page` of `p` into a fresh drawing, as its page window would show it (cable views and
/// reference texts included). Hold the project only for this call.
///
/// # Errors
/// Unknown page, or an item that cannot be drawn.
pub fn preview_drawing(p: &Project, page: PageId) -> Result<Drawing, String> {
    let doc = p.ledger.document();
    if !doc.pages.contains_key(&page) {
        return Err("The previewed page no longer exists.".into());
    }
    project_page_with(doc, page, &base_drawing(), &HandleMap::default(), &p.views, p.tolerance).map(|(d, _)| d)
}

/// Rasterise `drawing` at `view` into an image of `width` × `height` pixels on `background`, with
/// lineweights at `px_per_mm` pixels per millimetre (0: hairlines only).
///
/// # Errors
/// An unusable view or image size (0, or above 4096 pixels a side).
pub fn rasterise(
    drawing: &Drawing,
    view: PreviewView,
    width: u32,
    height: u32,
    background: [u8; 3],
    px_per_mm: f32,
) -> Result<PreviewImage, String> {
    if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) || !view.usable() {
        return Err("the preview cannot be drawn at this size or view".into());
    }
    let scale = view.scale(height);
    let rv = raster::View { center: Vec2::new(view.center.x, view.center.y), scale, width, height };
    let opts = pipeline::Options {
        tolerance: 0.5 / scale,
        min_dash: 2.0 / scale,
        lineweights: px_per_mm > 0.0,
        view_height: view.height,
        view: Some(rv.world_bounds()),
        ..pipeline::Options::default()
    };
    let list = pipeline::build(drawing, &Space::Model, &opts);
    let ro = raster::RasterOptions {
        background: cadcraft_engine::doc::color::Rgb(background[0], background[1], background[2]),
        hairline: 1.0,
        px_per_mm,
        antialias: true,
    };
    let pm = raster::render(&list, &rv, &ro).ok_or("the preview could not be rasterised")?;
    Ok(PreviewImage { width, height, rgba: pm.data().to_vec() })
}

/// The cable reference the selection `handles` on document `uid` names (its point or its text),
/// for Preview Reference.
pub fn selected_reference(p: &Project, uid: u64, handles: &[cadcraft_engine::doc::Handle]) -> Option<RepresentationId> {
    let page = p.page_of(uid)?;
    let map = &p.pages.get(&page)?.map;
    let doc = p.ledger.document();
    handles.iter().find_map(|h| match map.get(*h)? {
        crate::project::Mapped::RefText(r) => Some(r),
        crate::project::Mapped::Item(i) => {
            doc.representation_of_item(i).filter(|(_, r)| crate::references::is_reference(r)).map(|(id, _)| id)
        }
        _ => None,
    })
}
