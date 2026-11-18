//! The words and codes of cables and wires (design 065 §3, cut 3): core-label and colour-code
//! limits, IEC 60757 colour codes and the colours they draw in, shortened text, and which
//! two-colour wires on a page get their dashes. Pure rules over the document; the cable fan
//! drawing that uses them stays in `look`.

use ecad_model::colour::ConductorColour;
use ecad_model::wire::Colours;
use ecad_model::{Document, PageId, Pt, WireId};

/// Longest core label accepted (Unicode scalar values, after trimming).
pub const MAX_CORE_CHARS: usize = 32;

/// `text` as drawn: control characters become `?`, and text longer than `max` characters is cut
/// to `max - 1` characters and `…` (design §4.5). The stored value is never changed.
pub fn shorten(text: &str, max: usize) -> String {
    let clean: Vec<char> = text.chars().map(|c| if c.is_control() { '?' } else { c }).collect();
    if clean.len() <= max {
        return clean.into_iter().collect();
    }
    let mut out: String = clean.into_iter().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// `ecad.wire_colour.*` (design `040.md` §2): the colour each conductor colour draws in, on the
/// paper and the dark sheet, in [`ConductorColour::ALL`]'s order (so a colour's index is
/// `colour as usize`). Black draws in ink, so it shows on both sheets; white draws as a warm grey
/// on paper. A colour's words come from [`ConductorColour::forms`], never from here.
pub const CODE_COLOURS: [(ConductorColour, ecad_drawing_style::Color, ecad_drawing_style::Color); 14] = {
    use ConductorColour as K;
    use ecad_drawing_style::Color as C;
    [
        (K::Black, ecad_drawing_style::sheet::PAPER.ink, ecad_drawing_style::sheet::DARK.ink),
        (K::Brown, C::hex(0x8A4B1F), C::hex(0xC9895A)),
        (K::Red, C::hex(0xC62828), C::hex(0xFF7A70)),
        (K::Orange, C::hex(0xD96800), C::hex(0xFFA34D)),
        (K::Yellow, C::hex(0xE3B800), C::hex(0xF2D34B)),
        (K::Green, C::hex(0x2F8F3A), C::hex(0x6FD67A)),
        (K::Blue, C::hex(0x23408F), C::hex(0x8FB0FF)),
        (K::Violet, C::hex(0x7B3FA0), C::hex(0xC392F0)),
        (K::Grey, C::hex(0x7C8591), C::hex(0xA3ABB6)),
        (K::White, C::hex(0xB9B4A6), C::hex(0xF2F3F5)),
        (K::Pink, C::hex(0xD9528F), C::hex(0xFF9CC8)),
        (K::Gold, C::hex(0xA8851C), C::hex(0xE3C463)),
        (K::Turquoise, C::hex(0x14948F), C::hex(0x54D3CC)),
        (K::Silver, C::hex(0x98A0A8), C::hex(0xC9CED4)),
    ]
};

/// Longest colour code looked at (characters after trimming).
pub const MAX_CODE_CHARS: usize = 16;

/// The colours of a colour code: one colour (`BU`, `BLU`, `blue`), or two, the first solid and
/// the second laid over it as dashes (`GNYE`, `GN/YE`, `GRN/YEL`, `RDBU`, `YEBN`), as indexes into
/// [`CODE_COLOURS`]. Read by [`Colours::parse`], the one reader of colour words. `None` for a code
/// the app does not know (it draws in ink and raises an information check), for an empty one and
/// for three colours.
///
/// # Example
/// ```
/// use ecad_electrical::words::colour_code;
/// assert_eq!(colour_code("GNYE"), colour_code("grn/yel"));
/// assert!(colour_code("GNYE").is_some_and(|(_, dashes)| dashes.is_some()));
/// assert_eq!(colour_code("PURPLISH"), None);
/// ```
pub fn colour_code(code: &str) -> Option<(usize, Option<usize>)> {
    let c = code.trim();
    if c.is_empty() || c.chars().count() > MAX_CODE_CHARS || !c.is_ascii() {
        return None;
    }
    match Colours::parse(c)?.list() {
        [a] => Some((*a as usize, None)),
        [a, b] => Some((*a as usize, Some(*b as usize))),
        _ => None,
    }
}

/// How a wire of colours `colours` is drawn on `sheet`: its solid colour, and the dashes' colour
/// for two colours; `None` for three colours (drawn in ink).
pub fn wire_colours(colours: &Colours, sheet: &ecad_drawing_style::Sheet) -> Option<ecad_drawing_style::wire::WireColors> {
    let (a, b) = match colours.list() {
        [a] => (*a, None),
        [a, b] => (*a, Some(*b)),
        _ => return None,
    };
    Some(ecad_drawing_style::wire::WireColors { solid: drawn(a, sheet), dashes: b.map(|b| drawn(b, sheet)) })
}

/// The colour `c` is drawn in on `sheet`.
pub fn drawn(c: ConductorColour, sheet: &ecad_drawing_style::Sheet) -> ecad_drawing_style::Color {
    let dark = sheet.sheet == ecad_drawing_style::sheet::DARK.sheet;
    let (_, paper, on_dark) = CODE_COLOURS[c as usize];
    if dark { on_dark } else { paper }
}

/// The colour code a conductor's route is drawn in (brief 050b, design 050 §5.5): its wire
/// category's line colour (`wire`: the insulation colour, its own or the category's; `ink`: none;
/// or a code). `None` when it is drawn in ink.
pub fn conductor_colour(doc: &Document, c: WireId) -> Option<Colours> {
    crate::styles::wire::wire_look(doc, c).route_code()
}

/// [`conductor_colour`] with the project's categories read once (`looks`).
pub fn conductor_colour_in(doc: &Document, looks: &crate::styles::wire::WireLooks, c: WireId) -> Option<Colours> {
    looks.look(doc, c).route_code()
}

/// Most dashes [`dash_segments`] makes for one route.
pub const MAX_DASHES: usize = 4_000;

/// The dashes of a two-colour wire along `points`: `on` long, `off` apart, starting with a dash
/// at the first point (spec: 2 mm on, 2 mm off, `ecad_drawing_style::wire`). At most [`MAX_DASHES`];
/// none when a length is not positive and finite.
pub fn dash_segments(points: &[Pt], on: f64, off: f64) -> Vec<(Pt, Pt)> {
    let mut out = Vec::new();
    if !(on.is_finite() && off.is_finite() && on > 0.0 && off > 0.0) {
        return out;
    }
    let period = on + off;
    // How far along the dash pattern the current segment starts.
    let mut phase = 0.0_f64;
    for w in points.windows(2) {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        let len = a.dist(*b);
        if !len.is_finite() || len <= 0.0 {
            continue;
        }
        let at = |t: f64| Pt::new(a.x + (b.x - a.x) * t / len, a.y + (b.y - a.y) * t / len);
        // Distance along this segment of the next dash start.
        let mut t = if phase < on { -phase } else { period - phase };
        while t < len {
            let (s, e) = (t.max(0.0), (t + on).min(len));
            if e > s {
                out.push((at(s), at(e)));
                if out.len() >= MAX_DASHES {
                    return out;
                }
            }
            t += period;
        }
        phase = (phase + len) % period;
    }
    out
}

/// Most two-colour dashes one page draws (all its wires together). A hostile file could ask for
/// tens of millions; beyond this the remaining two-colour wires are drawn in their first colour
/// only, and Checks lists the page ([`dash_budget`], `checks::Kind::DashesLeftOut`).
pub const MAX_PAGE_DASHES: usize = 20_000;

/// An upper bound of the dashes [`dash_segments`] makes along `points` (`on` + `off` apart): per
/// segment its whole periods and two more. Saturates; a length that is not finite counts as
/// `usize::MAX`.
pub fn dash_count(points: &[Pt], on: f64, off: f64) -> usize {
    let period = on + off;
    if !(period.is_finite() && period > 0.0) {
        return 0;
    }
    let mut n: usize = 0;
    for w in points.windows(2) {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        let k = a.dist(*b) / period;
        if !k.is_finite() || k >= (MAX_PAGE_DASHES as f64) * 2.0 {
            return usize::MAX;
        }
        n = n.saturating_add(k as usize).saturating_add(2);
    }
    n
}

/// Which two-colour wires on `page` get their dashes, within [`MAX_PAGE_DASHES`] (and
/// [`MAX_DASHES`] per wire), in the page's route order; the rest are drawn in their first colour
/// only. The same answer for the projection and for Checks: it reads the document only.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DashBudget {
    pub dashed: std::collections::BTreeSet<ecad_model::RepresentationId>,
    /// The conductors whose dashes are left out, in route order.
    pub left_out: Vec<WireId>,
}

/// The [`DashBudget`] of `page`.
pub fn dash_budget(doc: &Document, page: PageId) -> DashBudget {
    let (on, off) = (f64::from(ecad_drawing_style::wire::DASH_ON_MM), f64::from(ecad_drawing_style::wire::DASH_OFF_MM));
    let mut out = DashBudget::default();
    let mut total: usize = 0;
    let looks = crate::styles::wire::WireLooks::new(doc);
    // Each route on the page (the page index), in the colour of the wire it stands for.
    for route in doc.routes_on(page) {
        let (Some(points), Some((rep, _, _)), Some(c)) =
            (doc.routes.get(&route).map(|x| x.points.clone()), crate::wires::route_item(doc, route), crate::wires::wire_of_route(doc, route))
        else {
            continue;
        };
        // Only two-colour codes have dashes.
        let Some(code) = conductor_colour_in(doc, &looks, c) else { continue };
        if code.list().len() != 2 {
            continue;
        }
        let n = dash_count(&points, on, off);
        if n > MAX_DASHES || total.saturating_add(n) > MAX_PAGE_DASHES {
            if !out.left_out.contains(&c) {
                out.left_out.push(c);
            }
            continue;
        }
        total += n;
        out.dashed.insert(rep);
    }
    out
}

/// The height the cable look is laid out for (label spacing, box pitch and padding, flags, and the
/// width [`stroke_label_width`] measures): the `CableText` text style's, in drawing units (brief
/// 066d: the cable look never has a text size of its own).
pub fn label_height(doc: &ecad_model::Document) -> f64 {
    crate::text_style::text_style(doc, crate::text_style::TextKind::CableText).height
}

/// `ecad.wire-weight`: bends, tails, label box frame, reference flag (wire section).
pub const WIRE_WEIGHT: u16 = 25;

/// The side of its reference point a reference end's flag sits on, pointing away from the cable's
/// wires (design 009 §7.2, with design 010 §6.2's direction u: the opposite of the mean direction in
/// which the member routes on the page run away from the point). The flat end faces the wires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagSide {
    /// Wires come from the left: the flag reaches right of the point (fixture R's page 1).
    Right,
    /// Wires come from the right: the flag reaches left of the point (fixture R's page 2).
    Left,
    /// Wires come from below: the flag sits above the point, pointing up.
    Up,
    /// Wires come from above: the flag sits below the point, pointing down.
    Down,
}

impl FlagSide {
    /// The side for direction `u` (away from the wires), by its larger component; a tie, a zero or
    /// a non-finite direction is horizontal, and `u.x = 0` horizontal is `Right` (design 010's
    /// left-aligned text when `u.x ≥ 0`).
    pub fn away(u: Pt) -> FlagSide {
        if !u.is_finite() || u.x.abs() >= u.y.abs() {
            if u.x < 0.0 { FlagSide::Left } else { FlagSide::Right }
        } else if u.y > 0.0 {
            FlagSide::Up
        } else {
            FlagSide::Down
        }
    }
}

/// A reference end's flag laid out at `at` around a text `w` wide, on `side`: the rectangle
/// `(lo, hi)`, where the text starts (bottom left, height `h`, [`label_height`]), the closed outline
/// (rectangle plus a point of depth h on `side`) and the attach point M (middle of the flat side).
/// The reference point stays inside the rectangle, h/2 from the flat side.
pub struct FlagShape {
    pub lo: Pt,
    pub hi: Pt,
    pub text_at: Pt,
    pub outline: Vec<Pt>,
    pub attach: Pt,
}

/// Lay out a flag (see [`FlagShape`]). Midpoints are `lo + (hi - lo) / 2`, finite for any finite
/// rectangle.
pub fn flag_shape(at: Pt, w: f64, side: FlagSide, h: f64) -> FlagShape {
    let (lo, hi, text_at) = match side {
        FlagSide::Right => (Pt::new(at.x - 0.5 * h, at.y - 0.5 * h), Pt::new(at.x + w + 0.5 * h, at.y + 1.5 * h), at),
        FlagSide::Left => (Pt::new(at.x - w - 0.5 * h, at.y - 0.5 * h), Pt::new(at.x + 0.5 * h, at.y + 1.5 * h), Pt::new(at.x - w, at.y)),
        FlagSide::Up => {
            (Pt::new(at.x - 0.5 * w - 0.5 * h, at.y - 0.5 * h), Pt::new(at.x + 0.5 * w + 0.5 * h, at.y + 1.5 * h), Pt::new(at.x - 0.5 * w, at.y))
        }
        FlagSide::Down => {
            (Pt::new(at.x - 0.5 * w - 0.5 * h, at.y - 1.5 * h), Pt::new(at.x + 0.5 * w + 0.5 * h, at.y + 0.5 * h), Pt::new(at.x - 0.5 * w, at.y - h))
        }
    };
    let xm = lo.x + 0.5 * (hi.x - lo.x);
    let ym = lo.y + 0.5 * (hi.y - lo.y);
    let (outline, attach) = match side {
        FlagSide::Right => (vec![lo, Pt::new(hi.x, lo.y), Pt::new(hi.x + h, ym), hi, Pt::new(lo.x, hi.y)], Pt::new(lo.x, ym)),
        FlagSide::Left => (vec![lo, Pt::new(hi.x, lo.y), hi, Pt::new(lo.x, hi.y), Pt::new(lo.x - h, ym)], Pt::new(hi.x, ym)),
        FlagSide::Up => (vec![lo, Pt::new(hi.x, lo.y), hi, Pt::new(xm, hi.y + h), Pt::new(lo.x, hi.y)], Pt::new(xm, lo.y)),
        FlagSide::Down => (vec![lo, Pt::new(xm, lo.y - h), Pt::new(hi.x, lo.y), hi, Pt::new(lo.x, hi.y)], Pt::new(xm, hi.y)),
    };
    FlagShape { lo, hi, text_at, outline, attach }
}

/// Typed coordinates beyond this (or not finite) are refused (design §9).
pub const MAX_COORD: f64 = 1e12;

/// Refusal of a point that is not finite or out of range.
pub const OUT_OF_RANGE: &str = "Point out of range.";

/// Most lines a hover description shows.
pub const MAX_HOVER_LINES: usize = 12;

/// The width of `text` in the stroke font at height `h` ([`label_height`]), as the cable look
/// measures it (`look::font_width` with CADCraft's fonts); 0 when not finite.
pub fn stroke_label_width(text: &str, h: f64, m: &dyn crate::text_style::fonts::TextMetrics) -> f64 {
    let w = m.width(m.stroke_font(), text, h);
    if w.is_finite() && w >= 0.0 { w } else { 0.0 }
}

/// The box a reference end's flag `w` wide covers at `at` on `side`, its point included: what a
/// pick of the marker hits (brief 010's pick-first rule; `look::marker_box`).
pub fn flag_box(at: Pt, w: f64, side: FlagSide, h: f64) -> (Pt, Pt) {
    let f = flag_shape(at, w, side, h);
    let lo = f.outline.iter().fold(f.lo, |a, p| Pt::new(a.x.min(p.x), a.y.min(p.y)));
    let hi = f.outline.iter().fold(f.hi, |a, p| Pt::new(a.x.max(p.x), a.y.max(p.y)));
    (lo, hi)
}

/// Most handles (or objects) one call may name.
pub const MAX_HANDLES: usize = 100_000;

/// One Properties row (brief 019, `crate::props`): a label and a value box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The property key it edits (`number`, `title`, `description`…).
    pub key: String,
    pub label: String,
    /// The value shown.
    pub value: String,
    pub editable: bool,
    /// The value comes from the project (a page's Function or Location): shown faint.
    pub inherited: bool,
    /// Several selected objects differ: `*varies*`.
    pub varies: bool,
    /// Suggestions (a combo): Status, Sheet; a function's or location's codes from the list.
    pub choices: Vec<String>,
    /// Brief 052 (design 052 §9.1): an unset function or location's proposal from the selected
    /// symbol's context: the code, and where it comes from (`From the box on 105`). Shown as a
    /// **Use +CP1** link that sets it.
    pub proposal: Option<(String, String)>,
}

impl Row {
    /// A read-only row.
    pub fn ro(key: &str, label: &str, value: String) -> Row {
        Row { key: key.into(), label: label.into(), value, editable: false, inherited: false, varies: false, choices: Vec::new(), proposal: None }
    }
    /// An editable row.
    pub fn rw(key: &str, label: &str, value: String) -> Row {
        Row { editable: true, ..Row::ro(key, label, value) }
    }
}
