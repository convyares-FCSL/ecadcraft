//! The words and codes of cables and wires (design 065 §3, cut 3): core-label and colour-code
//! limits, IEC 60757 colour codes and the colours they draw in, shortened text, and which
//! two-colour wires on a page get their dashes. Pure rules over the document; the cable fan
//! drawing that uses them stays in `look`.

use ecad_model::{ConductorId, Document, ItemKind, PageId, Pt, Subject};

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

/// `ecad.wire_colour.*` (design `040.md` §2): the colour each IEC 60757 code draws in, on the
/// paper and the dark sheet. Black draws in ink, so it shows on both sheets; white draws as a warm
/// grey on paper. Codes as `ecad_model::wiring::COLOUR_CODES` (the single colours).
pub const CODE_COLOURS: [(&str, &str, ecad_drawing_style::Color, ecad_drawing_style::Color); 14] = {
    use ecad_drawing_style::Color as C;
    [
        ("BK", "BLK", ecad_drawing_style::sheet::PAPER.ink, ecad_drawing_style::sheet::DARK.ink),
        ("BN", "BRN", C::hex(0x8A4B1F), C::hex(0xC9895A)),
        ("RD", "RED", C::hex(0xC62828), C::hex(0xFF7A70)),
        ("OG", "ORG", C::hex(0xD96800), C::hex(0xFFA34D)),
        ("YE", "YEL", C::hex(0xE3B800), C::hex(0xF2D34B)),
        ("GN", "GRN", C::hex(0x2F8F3A), C::hex(0x6FD67A)),
        ("BU", "BLU", C::hex(0x23408F), C::hex(0x8FB0FF)),
        ("VT", "VIO", C::hex(0x7B3FA0), C::hex(0xC392F0)),
        ("GY", "GRY", C::hex(0x7C8591), C::hex(0xA3ABB6)),
        ("WH", "WHT", C::hex(0xB9B4A6), C::hex(0xF2F3F5)),
        ("PK", "PNK", C::hex(0xD9528F), C::hex(0xFF9CC8)),
        ("GD", "GLD", C::hex(0xA8851C), C::hex(0xE3C463)),
        ("TQ", "TQS", C::hex(0x14948F), C::hex(0x54D3CC)),
        ("SR", "SLR", C::hex(0x98A0A8), C::hex(0xC9CED4)),
    ]
};

/// Longest colour code looked at (characters after trimming).
pub const MAX_CODE_CHARS: usize = 16;

/// Index into [`CODE_COLOURS`] of one colour code, either form, any case.
fn one_code(s: &str) -> Option<usize> {
    CODE_COLOURS.iter().position(|(iec, three, _, _)| s.eq_ignore_ascii_case(iec) || s.eq_ignore_ascii_case(three))
}

/// The colours of a colour code: one colour (`BU`, `BLU`), or two, the first solid and the second
/// laid over it as dashes (`GNYE`, `GN/YE`, `GRN/YEL`, `RDBU`, `YEBN`). `None` for a code the app
/// does not know (it draws in ink and raises an information check) and for an empty one.
///
/// # Example
/// ```
/// use ecad_cadcraft::look::colour_code;
/// assert_eq!(colour_code("GNYE"), colour_code("grn/yel"));
/// assert!(colour_code("GNYE").is_some_and(|(_, dashes)| dashes.is_some()));
/// assert_eq!(colour_code("PURPLISH"), None);
/// ```
pub fn colour_code(code: &str) -> Option<(usize, Option<usize>)> {
    let c = code.trim();
    if c.is_empty() || c.chars().count() > MAX_CODE_CHARS || !c.is_ascii() {
        return None;
    }
    if let Some(i) = one_code(c) {
        return Some((i, None));
    }
    let parts: Vec<&str> = c.split(['/', '-', ' ', '+']).filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [a, b] => Some((one_code(a)?, Some(one_code(b)?))),
        // Run together: two two-letter codes (`GNYE`) or two three-letter ones (`GRNYEL`).
        [ab] => [2usize, 3].iter().find_map(|n| {
            let (a, b) = (ab.get(..*n)?, ab.get(*n..)?);
            if b.len() != *n {
                return None;
            }
            Some((one_code(a)?, Some(one_code(b)?)))
        }),
        _ => None,
    }
}

/// How a wire of colour `code` is drawn on `sheet`: its solid colour, and the dashes' colour for a
/// two-colour code; `None` for an unknown code (drawn in ink).
pub fn wire_colours(code: &str, sheet: &ecad_drawing_style::Sheet) -> Option<ecad_drawing_style::wire::WireColors> {
    let dark = sheet.sheet == ecad_drawing_style::sheet::DARK.sheet;
    let colour = |i: usize| CODE_COLOURS.get(i).map(|(_, _, paper, on_dark)| if dark { *on_dark } else { *paper });
    let (a, b) = colour_code(code)?;
    Some(ecad_drawing_style::wire::WireColors { solid: colour(a)?, dashes: b.and_then(colour) })
}

/// The colour code a conductor's route is drawn in (brief 050b, design 050 §5.5): its wire
/// category's line colour (`wire`: the insulation colour, its own or the category's; `ink`: none;
/// or a code). `None` when it is drawn in ink.
pub fn conductor_colour(doc: &Document, c: ConductorId) -> Option<String> {
    crate::styles::wire::wire_look(doc, c).route_code()
}

/// [`conductor_colour`] with the project's categories read once (`looks`).
pub fn conductor_colour_in(doc: &Document, looks: &crate::styles::wire::WireLooks, c: ConductorId) -> Option<String> {
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
    pub left_out: Vec<ConductorId>,
}

/// The [`DashBudget`] of `page`.
pub fn dash_budget(doc: &Document, page: PageId) -> DashBudget {
    let (on, off) = (f64::from(ecad_drawing_style::wire::DASH_ON_MM), f64::from(ecad_drawing_style::wire::DASH_OFF_MM));
    let mut out = DashBudget::default();
    let mut total: usize = 0;
    let looks = crate::styles::wire::WireLooks::new(doc);
    for (rep, r) in doc.representations_on(page) {
        let Subject::Conductor(c) = r.subject else { continue };
        if r.form != ecad_model::Form::Drawn {
            continue;
        }
        // Only two-colour codes have dashes.
        let Some(code) = conductor_colour_in(doc, &looks, c) else { continue };
        if !colour_code(&code).is_some_and(|(_, b)| b.is_some()) {
            continue;
        }
        let Some(ItemKind::Primitive(prim)) = r.items.first().and_then(|i| doc.items.get(i)).map(|i| &i.kind) else { continue };
        let Some(points) = crate::wires::path_vertices(prim) else { continue };
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
