//! The engine half of how a cable looks on a page (brief 009, design 009): text measured with the
//! drafting font CADCraft draws text with ([`font_width`]), the box a reference end's marker covers
//! ([`marker_box`]), and the canvas: the sheet in use, coloured routes and painted marks.
//!
//! The fan, the marker and the core labels themselves are computed over the document in
//! [`ecad_electrical::cables::layout`], with [`font_width`] passed in to measure text.
//!
//! This module records nothing in the document.

use ecad_model::{Document, ItemKind, PageId, Pt, Subject, WireId};

pub use ecad_electrical::cables::words::{
    CODE_COLOURS, DashBudget, MAX_CODE_CHARS, MAX_CORE_CHARS, MAX_DASHES, MAX_PAGE_DASHES, colour_code, conductor_colour, conductor_colour_in,
    dash_budget, dash_count, dash_segments, shorten, wire_colours,
};
pub use ecad_electrical::cables::words::{FlagShape, FlagSide, WIRE_WEIGHT, flag_shape, label_height};
/// The representation role of a cable's reference end: brief 010's, one definition for both briefs.
pub use ecad_model::connectivity::derive::MAX_REFERENCES_PER_CABLE;
/// The width of `text` per unit of text height in the drafting font CADCraft draws text with (0
/// for anything that does not measure): multiply by the height it is drawn at.
pub fn font_width(text: &str) -> f64 {
    let w = cadcraft_engine::fonts::line_width(text, 1.0, 1.0);
    if w.is_finite() && w >= 0.0 { w } else { 0.0 }
}

/// The box a reference end's marker covers on the page (its flag, point included), for a
/// reference end at `at` showing `text` with its flag on `side`: what a pick of the marker hits
/// (brief 010's pick-first rule). It follows the flag to whichever side it is drawn.
pub fn marker_box(at: Pt, text: &str, side: FlagSide, h: f64) -> (Pt, Pt) {
    ecad_electrical::cables::words::flag_box(at, font_width(&shorten(text, 64)) * h, side, h)
}

// ---------------------------------------------------------------- the canvas (brief 040)

/// The dark sheet is in use (brief 040, step 1): a viewing preference of the person, set by the
/// app, never stored in the project. Process-wide, as there is one window.
static DARK_SHEET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Use the dark sheet (`true`) or the paper sheet (`false`, the default) for ECAD's overlay marks.
pub fn set_dark_sheet(dark: bool) {
    DARK_SHEET.store(dark, std::sync::atomic::Ordering::Relaxed);
}

/// The sheet in use: [`ecad_drawing_style::sheet::PAPER`] unless [`set_dark_sheet`] chose the dark one.
pub fn sheet() -> &'static ecad_drawing_style::Sheet {
    if DARK_SHEET.load(std::sync::atomic::Ordering::Relaxed) { &ecad_drawing_style::sheet::DARK } else { &ecad_drawing_style::sheet::PAPER }
}

thread_local! {
    /// A sheet the projection uses instead of the one in use ([`with_paper`]: printing).
    static PROJECTION_SHEET: std::cell::Cell<Option<&'static ecad_drawing_style::Sheet>> = const { std::cell::Cell::new(None) };
}

/// The sheet a page is projected for: [`sheet`], or paper while [`with_paper`] runs.
pub fn projection_sheet() -> &'static ecad_drawing_style::Sheet {
    PROJECTION_SHEET.with(std::cell::Cell::get).unwrap_or_else(sheet)
}

/// Keeps the sheet [`pin_sheet`] read for this thread until it is dropped.
#[derive(Debug)]
pub struct SheetPin(Option<&'static ecad_drawing_style::Sheet>);

impl Drop for SheetPin {
    fn drop(&mut self) {
        PROJECTION_SHEET.with(|c| c.set(self.0));
    }
}

/// Read the sheet once and keep it for this thread until the pin drops: a page's projection and
/// its check then agree on every colour. Without it, a sheet switch on another thread
/// (`canvassheet`, which is process-wide) between the projection and the check made a rebuilt
/// page fail its check and become unavailable (CI on PR 88, `history_ui`).
pub fn pin_sheet() -> SheetPin {
    let s = projection_sheet();
    SheetPin(PROJECTION_SHEET.with(|c| c.replace(Some(s))))
}

/// Run `f` with pages projected for the paper sheet (a print is on white paper, whatever sheet
/// the screen shows).
pub fn with_paper<R>(f: impl FnOnce() -> R) -> R {
    let before = PROJECTION_SHEET.with(|c| c.replace(Some(&ecad_drawing_style::sheet::PAPER)));
    let r = f();
    PROJECTION_SHEET.with(|c| c.set(before));
    r
}

/// The selection with every wire dash replaced by its wire's route (picking a dash picks the
/// wire), or `None` when it holds no dash.
pub fn pick_routes(p: &crate::Project, page: PageId, selection: &[cadcraft_engine::doc::Handle]) -> Option<Vec<cadcraft_engine::doc::Handle>> {
    let map = &p.pages.get(&page)?.map;
    if !selection.iter().any(|h| map.route_of_dash(*h).is_some()) {
        return None;
    }
    let mut out: Vec<cadcraft_engine::doc::Handle> = Vec::with_capacity(selection.len());
    for h in selection {
        let h = map.route_of_dash(*h).unwrap_or(*h);
        if !out.contains(&h) {
            out.push(h);
        }
    }
    Some(out)
}

/// The commands of this module with a typed input.
pub const TYPED: &[crate::commands::input::Typed] = &[crate::commands::input::Typed::of::<CanvassheetParams>("canvassheet")];

/// The commands this module adds.
pub fn specs() -> Vec<cadcraft_engine::CommandSpec> {
    vec![
        cadcraft_engine::CommandSpec::new("canvassheet", "Canvas Sheet", crate::commands::input::typed::<CanvassheetParams>)
            .params("{dark: bool} (display only: the sheet the page is drawn on)")
            .noundo(),
    ]
}

/// `canvassheet {dark}` (brief 040): the sheet the app shows, for the colours ECAD draws in; the
/// open page is redrawn when it changes. Display only, nothing recorded.
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CanvassheetParams {
    /// The dark sheet (true) or the paper one (false).
    dark: bool,
}

impl crate::commands::input::Command for CanvassheetParams {
    fn run(self, s: &mut cadcraft_engine::Session) -> cadcraft_engine::Result<serde_json::Value> {
        let dark = self.dark;
        let changed = DARK_SHEET.swap(dark, std::sync::atomic::Ordering::Relaxed) != dark;
        if changed {
            // The open page only: there is one page window, and a page is projected afresh when shown.
            let _ = crate::commands::on_any_page(s, |p, pg| {
                p.redraw_window(pg, Some("canvassheet"));
                Ok(())
            });
        }
        Ok(serde_json::json!({"dark": dark, "changed": changed}))
    }
}

/// A token colour as CADCraft's RGB.
pub fn rgb(c: ecad_drawing_style::Color) -> cadcraft_engine::doc::color::Rgb {
    cadcraft_engine::doc::color::Rgb(c.r, c.g, c.b)
}

/// A wire route drawn in its colours on a page (brief 040, step 3).
#[derive(Debug, Clone, PartialEq)]
pub struct ColouredRoute {
    pub conductor: WireId,
    /// The route representation.
    pub rep: ecad_model::RepresentationId,
    /// The handle the route is drawn with on the page (to leave a selected wire to CADCraft).
    pub handle: Option<cadcraft_engine::doc::Handle>,
    pub points: Vec<Pt>,
    pub colours: ecad_drawing_style::wire::WireColors,
}

/// Most routes [`coloured_routes`] returns for one page.
pub const MAX_COLOURED_ROUTES: usize = 20_000;

/// The wire routes of `page` whose colour code is known, with their colours on `sheet`. Routes
/// not drawn (a cable in fan view hides its members) and plain black wires (ink already) are left
/// out. `map`: the page's handles when it is projected (routes it does not draw are left out).
/// Display and print only; nothing is stored.
pub fn coloured_routes(
    doc: &Document, page: PageId, map: Option<&crate::project::HandleMap>, sheet: &ecad_drawing_style::Sheet,
) -> Vec<ColouredRoute> {
    let mut out = Vec::new();
    let draws = ecad_electrical::styles::wire::WireDraws::new(doc);
    for (rep, r) in doc.representations_on(page) {
        let Subject::Route(x) = r.subject else { continue };
        if r.form != ecad_model::Form::Drawn {
            continue;
        }
        // A line is coloured as the wire it stands for (design 072 §4.3).
        let Some(c) = ecad_electrical::wires::wire_of_route(doc, x) else { continue };
        let Some(item) = r.items.first() else { continue };
        let handle = map.and_then(|m| m.item(*item));
        if map.is_some() && handle.is_none() {
            continue;
        }
        let Some(ItemKind::Primitive(prim)) = doc.items.get(item).map(|i| &i.kind) else { continue };
        let Some(points) = ecad_electrical::wires::path_vertices(prim) else { continue };
        if points.len() < 2 || !points.iter().all(|q| q.x.is_finite() && q.y.is_finite()) {
            continue;
        }
        let Some(colours) = conductor_colour_in(doc, &draws, c).and_then(|code| wire_colours(&code, sheet)) else { continue };
        if colours.solid == sheet.ink && colours.dashes.is_none() {
            continue;
        }
        out.push(ColouredRoute { conductor: c, rep, handle, points, colours });
        if out.len() >= MAX_COLOURED_ROUTES {
            break;
        }
    }
    out
}

/// A marker ECAD paints over the page in a shape CADCraft's marks lack (brief 040, step 2):
/// display only, never an entity, so it never reaches a plot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Painted {
    /// An error (an orphan cable reference): a filled disc with "!".
    Error(Pt),
    /// A warning (an open end that looks connected): a filled triangle with "!".
    Warning(Pt),
}

/// The painted markers of page drawing `uid`.
pub fn painted_marks(p: &crate::Project, uid: u64) -> Vec<Painted> {
    let Some(page) = p.page_of(uid) else { return Vec::new() };
    let Some(view) = p.pages.get(&page).filter(|v| !v.unavailable) else { return Vec::new() };
    let doc = p.ledger.document();
    let mut out: Vec<Painted> =
        ecad_electrical::references::cables::markers_on_page(doc, page).into_iter().filter(|m| m.orphan).map(|m| Painted::Error(m.at)).collect();
    out.extend(view.map.looks_connected().iter().map(|at| Painted::Warning(*at)));
    out
}

/// The handles of the provisional device tags drawn on `page` (`-K?1`): painted in the
/// provisional colour with a dotted underline (spec §5, "dotted means not final yet").
pub fn provisional_tag_handles(p: &crate::Project, page: PageId) -> Vec<cadcraft_engine::doc::Handle> {
    let doc = p.ledger.document();
    let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return Vec::new() };
    map.tag_text_handles()
        .filter(|h| {
            let Some((rep, ecad_electrical::devices::TextOf::Tag)) = map.tag_text_of(*h) else { return false };
            let Some(Subject::Device(c)) = doc.representations.get(&rep).map(|r| r.subject) else { return false };
            ecad_electrical::devices::tag_of(doc, c).is_some_and(ecad_model::naming::tag::is_provisional)
        })
        .collect()
}

/// A text style's colour as drawn on `sheet` (brief 050a): its own colour, or its colour code's
/// first colour on that sheet, as [`colour_rgb`] draws the stored value.
pub fn text_colour_rgb(
    c: ecad_electrical::styles::text_style::TextColour, sheet: &ecad_drawing_style::Sheet,
) -> Option<cadcraft_engine::doc::color::Rgb> {
    match c {
        ecad_electrical::styles::text_style::TextColour::Rgb(r, g, b) => Some(cadcraft_engine::doc::color::Rgb(r, g, b)),
        ecad_electrical::styles::text_style::TextColour::Code(i) => {
            let dark = sheet.sheet == ecad_drawing_style::sheet::DARK.sheet;
            ecad_electrical::cables::words::CODE_COLOURS.get(i).map(|(_, paper, on_dark)| rgb(if dark { *on_dark } else { *paper }))
        }
    }
}

/// A colour field's value as drawn on `sheet`: `None` for `none` (ink) and anything unknown.
pub fn colour_rgb(value: &str, sheet: &ecad_drawing_style::Sheet) -> Option<cadcraft_engine::doc::color::Rgb> {
    let t = value.trim();
    if t.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some((r, g, b)) = ecad_electrical::styles::hex_rgb(t) {
        return Some(cadcraft_engine::doc::color::Rgb(r, g, b));
    }
    let w = ecad_electrical::cables::words::wire_colours(&ecad_model::objects::wire::Colours::parse(t)?, sheet)?;
    Some(crate::cables::draw::rgb(w.solid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortening_counts_characters_and_hides_controls() {
        assert_eq!(shorten("W1", 24), "W1");
        assert_eq!(shorten(&"x".repeat(25), 24), format!("{}…", "x".repeat(23)));
        assert_eq!(shorten("a\nb", 24), "a?b");
    }
}
