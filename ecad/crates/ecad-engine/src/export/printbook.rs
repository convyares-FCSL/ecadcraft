//! Print Book: every page in scope, generated pages included, as one PDF
//! on CADCraft's plot path. ECAD writes no PDF itself.
//!
//! # Semantics
//! - The pages in scope are resolved in book order ([`resolve`]): all pages, a folder (with its
//!   subfolders) or a page list (`100, 105-110`; a range runs in book order).
//! - Each page is projected as the page window draws it (frame, title block, derived texts, cable
//!   views, generated content), on a **scratch** drawing that no session owns, each sheet placed
//!   to the right of the one before. The engine's installed plot hook is then called once per
//!   volume with one `{window, paper, landscape}` per sheet and zero margins, as `pdfbook` does.
//! - Paper: the smallest ISO A size a sheet fits; a sheet that is not exactly that size is printed
//!   fitted and said so. More than [`MAX_SHEETS`] sheets are written as volumes `name.pdf`,
//!   `name-2.pdf`, … breaking between pages where possible.
//! - Printing is a one-off on request: it reads the project (a full build is allowed here) and
//!   records nothing.
//!
//! Excluded: printing to a physical printer, A4 and portrait sheets.

use std::collections::BTreeSet;

use cadcraft_engine::doc::{Drawing, EntityStore, color::Color};
use cadcraft_engine::geom::{Mat3, Vec2};
use ecad_model::document::book::{self, Book};
use ecad_model::{CableId, Document, FolderId, PageId, Pt};
use serde_json::{Value, json};

use crate::project::{HandleMap, base_drawing, project_page_framed};
use ecad_electrical::reports::kept::ReportModels;

/// Most sheets in one PDF (CADCraft's `MAX_PAGES`).
pub(crate) const MAX_SHEETS: usize = 256;
/// Gap between pages on the scratch drawing, in millimetres.
const GAP: f64 = 50.0;
/// Said when no plotter is installed.
pub(crate) const NO_PLOTTER: &str = "Printing is not available in this build (no PDF plotter is installed).";

/// What to print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    All,
    Folder(FolderId),
    /// Page numbers and ranges, comma-separated.
    Pages(String),
}

/// The pages a scope names, in book order, and the numbers it named that no page has.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Resolved {
    pub pages: Vec<PageId>,
    pub unknown: Vec<String>,
    /// Notes on what was chosen (a continuation sheet's number prints its whole page).
    pub notes: Vec<String>,
}

fn folder_pages(doc: &Document, f: FolderId, out: &mut BTreeSet<PageId>) {
    out.extend(doc.pages_in_folder(f));
    for sub in doc.subfolders(f) {
        folder_pages(doc, sub, out);
    }
}

/// The pages `scope` names, in book order.
///
/// # Errors
/// An unknown folder.
pub fn resolve(doc: &Document, scope: &Scope) -> Result<Resolved, String> {
    resolve_with(doc, scope, None)
}

/// [`resolve`] knowing the generated pages' sheets (`reports`): a continuation sheet's number
/// (`401`, sheet 2 of page `400`) chooses its whole page, with a note.
///
/// # Errors
/// An unknown folder.
pub fn resolve_with(doc: &Document, scope: &Scope, reports: Option<&ReportModels>) -> Result<Resolved, String> {
    let b = Book::of(doc);
    match scope {
        Scope::All => Ok(Resolved { pages: b.order.clone(), ..Resolved::default() }),
        Scope::Folder(f) => {
            if !doc.folders().contains_key(f) {
                return Err(ecad_electrical::CmdError::NotFound(ecad_model::Noun::Folder).to_string());
            }
            let mut set = BTreeSet::new();
            folder_pages(doc, *f, &mut set);
            Ok(Resolved { pages: b.order.iter().copied().filter(|p| set.contains(p)).collect(), ..Resolved::default() })
        }
        Scope::Pages(text) => {
            let numbers: Vec<String> = b.order.iter().map(|p| b.number(doc, *p)).collect();
            let find = |n: &str| numbers.iter().position(|x| x.eq_ignore_ascii_case(n.trim()));
            let mut keep = BTreeSet::new();
            let mut unknown = Vec::new();
            let mut notes = Vec::new();
            for token in text.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                if let Some(i) = find(token) {
                    keep.insert(i);
                    continue;
                }
                if let Some((owner, i)) = ecad_electrical::pages::generated::continuation_of(doc, &b, token, reports)
                    && let Some(at) = b.position(owner).and_then(|n| n.checked_sub(1))
                {
                    keep.insert(at);
                    notes.push(format!("{token} is sheet {} of page {}; the whole page is printed.", i + 1, b.number(doc, owner)));
                    continue;
                }
                match token.split_once('-').map(|(a, z)| (find(a), find(z), a, z)) {
                    Some((Some(i), Some(j), _, _)) => keep.extend(i.min(j)..=i.max(j)),
                    Some((i, j, a, z)) => {
                        if i.is_none() {
                            unknown.push(a.trim().to_string());
                        }
                        if j.is_none() {
                            unknown.push(z.trim().to_string());
                        }
                    }
                    None => unknown.push(token.to_string()),
                }
            }
            Ok(Resolved { pages: keep.into_iter().filter_map(|i| b.order.get(i).copied()).collect(), unknown, notes })
        }
    }
}

/// The ISO A paper a `w` x `h` mm sheet prints on, whether it fits exactly, and landscape.
pub(crate) fn paper_for(w: f64, h: f64) -> (&'static str, bool, bool) {
    let (long, short) = (w.max(h), w.min(h));
    let papers = [("A4", 297.0, 210.0), ("A3", 420.0, 297.0), ("A2", 594.0, 420.0), ("A1", 841.0, 594.0), ("A0", 1189.0, 841.0)];
    for (name, l, s) in papers {
        if long <= l + 0.5 && short <= s + 0.5 {
            let exact = (long - l).abs() <= 0.5 && (short - s).abs() <= 0.5;
            return (name, exact, w >= h);
        }
    }
    ("A0", false, w >= h)
}

/// One printed sheet on the scratch drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub page: PageId,
    pub lo: Pt,
    pub hi: Pt,
    pub paper: &'static str,
    pub landscape: bool,
    pub exact: bool,
}

/// The scratch drawing of `pages` and its sheets, in book order.
///
/// # Errors
/// A page that cannot be projected.
pub fn build(
    doc: &Document, pages: &[PageId], fan: &BTreeSet<CableId>, tol: f64, reports: Option<&ReportModels>,
) -> Result<(Drawing, Vec<Sheet>), String> {
    build_pending(doc, pages, fan, tol, reports, false)
}

/// [`build`] with the project's pending flag: the title blocks and the cover
/// show the pending marker.
///
/// # Errors
/// A page that cannot be projected.
pub fn build_pending(
    doc: &Document, pages: &[PageId], fan: &BTreeSet<CableId>, tol: f64, reports: Option<&ReportModels>, pending: bool,
) -> Result<(Drawing, Vec<Sheet>), String> {
    build_in(doc, pages, fan, tol, reports, pending, false)
}

/// [`build_pending`] for a print in `colour` (`print.colour`): wires print in
/// their colours (the paper sheet's), a two-colour wire with the second colour's dashes over the
/// first, and everything else in ink, as the "Prints" column of the print table says (cable, provisional
/// text and terminal links in ink). Without `colour` (mono) the wires are ink and their dashes
/// are left out; the colour code is in the wire label.
///
/// # Errors
/// A page that cannot be projected.
pub fn build_in(
    doc: &Document, pages: &[PageId], fan: &BTreeSet<CableId>, tol: f64, reports: Option<&ReportModels>, pending: bool, colour: bool,
) -> Result<(Drawing, Vec<Sheet>), String> {
    let mut scratch = base_drawing();
    // The scratch drawing carries ECAD's text styles (Standard, ECAD-BOLD).
    crate::styles::draw::install(&mut scratch, doc);
    crate::locations::draw::add_linetype(&mut scratch);
    let mut store = EntityStore::new();
    let mut sheets = Vec::new();
    let mut x = 0.0;
    for page in pages {
        let parts = crate::pages::generated::parts_of_pending(doc, *page, reports, pending);
        // A print is on white paper: wire colours for the paper sheet.
        let (d, map) = crate::cables::draw::with_paper(|| project_page_framed(doc, *page, &base_drawing(), &HandleMap::default(), fan, tol, &parts))?;
        for l in &d.layers {
            if scratch.layer(&l.name).is_none() {
                scratch.layers.push(l.clone());
            }
        }
        for lt in &d.linetypes {
            if scratch.linetype(&lt.name).is_none() {
                scratch.linetypes.push(lt.clone());
            }
        }
        // The text styles of objects' own fonts (`ECAD-FONT-…`).
        for ts in &d.text_styles {
            if !scratch.text_styles.iter().any(|x| x.name.eq_ignore_ascii_case(&ts.name)) {
                scratch.text_styles.push(ts.clone());
            }
        }
        for (name, b) in &d.blocks {
            scratch.blocks.entry(name.clone()).or_insert_with(|| b.clone());
        }
        let m = Mat3::translate(Vec2::new(x, 0.0));
        for e in d.model.iter() {
            // Mono leaves a two-colour wire's dashes out (ink on ink).
            if !colour && e.common.layer == ecad_electrical::wires::DASH_LAYER {
                continue;
            }
            let mut e = (**e).clone();
            e.kind.transform(&m);
            // A missing value prints black in either colour mode.
            if e.common.layer == ecad_electrical::pages::generated::LAYER
                && e.common.color == Color::Index(ecad_electrical::pages::generated::FLAGGED_ACI)
            {
                e.common.color = Color::Index(ecad_electrical::pages::generated::FLAGGED_PRINT_ACI);
            }
            // Colour: everything but the wire colours prints in ink. Mono: the wires print in ink.
            // A style's colour (text, location box, frame; set as a true colour by
            // the projection) prints like a wire colour: in a colour print only.
            let wire_colour =
                e.common.layer == ecad_electrical::wires::DASH_LAYER || map.is_coloured(e.handle) || matches!(e.common.color, Color::True(_));
            if colour != wire_colour {
                e.common.color = Color::Index(7);
            }
            e.handle = scratch.new_handle();
            store.push(e);
        }
        let windows = ecad_electrical::pages::generated::sheet_windows_of(doc, *page, reports);
        let mut right = x;
        for (lo, hi) in windows {
            let (paper, exact, landscape) = paper_for(hi.x - lo.x, hi.y - lo.y);
            sheets.push(Sheet { page: *page, lo: Pt::new(lo.x + x, lo.y), hi: Pt::new(hi.x + x, hi.y), paper, landscape, exact });
            right = right.max(hi.x + x);
        }
        x = right + GAP;
    }
    scratch.model = store;
    Ok((scratch, sheets))
}

/// Volumes of at most [`MAX_SHEETS`] sheets, breaking between pages where possible.
pub(crate) fn volumes(sheets: &[Sheet]) -> Vec<&[Sheet]> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < sheets.len() {
        let end = (start + MAX_SHEETS).min(sheets.len());
        // Back up to a page boundary when the cut falls inside a page.
        let mut cut = end;
        if end < sheets.len() {
            while cut > start + 1 && sheets.get(cut).map(|s| s.page) == sheets.get(cut - 1).map(|s| s.page) {
                cut -= 1;
            }
            if cut == start + 1 && sheets.get(cut).map(|s| s.page) == sheets.get(start).map(|s| s.page) {
                cut = end;
            }
        }
        if let Some(v) = sheets.get(start..cut) {
            out.push(v);
        }
        start = cut;
    }
    out
}

/// Print options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Black on white (`monochrome.ctb`), else as on screen.
    pub mono: bool,
    pub lineweights: bool,
    /// The drawing changed since the newest issue, so the title
    /// blocks and cover show the pending marker and the notes say so.
    pub pending: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { mono: true, lineweights: true, pending: false }
    }
}

/// PDF bytes of `sheets` of `scratch` through the engine's installed plot hook.
///
/// # Errors
/// No plotter installed, or the plotter's error.
pub fn plot(scratch: &Drawing, sheets: &[Sheet], o: Options) -> Result<Vec<u8>, String> {
    let hook = cadcraft_engine::cmd::file::io().and_then(|h| h.plot).ok_or(NO_PLOTTER)?;
    let pages: Vec<Value> =
        sheets.iter().map(|s| json!({ "window": [[s.lo.x, s.lo.y], [s.hi.x, s.hi.y]], "paper": s.paper, "landscape": s.landscape })).collect();
    let mut opts = json!({ "layout": "Model", "pages": pages, "margins": [0, 0, 0, 0], "lineweights": o.lineweights });
    if o.mono {
        opts["plotStyleTable"] = json!("monochrome.ctb");
    }
    hook(scratch, &cadcraft_engine::doc::Space::Model, &opts)
}

/// The file of volume `i` (0-based) of `path`: `demo.pdf`, `demo-2.pdf`, …
pub(crate) fn volume_path(path: &str, i: usize) -> String {
    if i == 0 {
        return path.to_string();
    }
    let p = std::path::Path::new(path);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "book".into());
    let ext = p.extension().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "pdf".into());
    p.with_file_name(format!("{stem}-{}.{ext}", i + 1)).to_string_lossy().into_owned()
}

/// Base64 of `bytes` (standard alphabet, padded), as `plot` returns PDF data.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk.first().copied().unwrap_or(0), chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(A.get(((n >> (18 - 6 * i)) & 63) as usize).copied().unwrap_or(b'=')));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// What a print wrote or returned.
#[derive(Debug, Clone, Default)]
pub struct Printed {
    /// (path or `None`, sheets, bytes, PDF bytes when not written).
    pub files: Vec<(Option<String>, usize, usize, Option<Vec<u8>>)>,
    pub sheets: usize,
    pub generated: usize,
    pub flagged: usize,
    /// `Page 120 (500 x 300) is printed fitted on A2.`
    pub notes: Vec<String>,
}

/// Print `pages` of `doc` to `path` (or return the PDF bytes).
///
/// # Errors
/// A projection, plot or write failure; nothing is written then.
pub fn print(
    doc: &Document, pages: &[PageId], fan: &BTreeSet<CableId>, tol: f64, reports: Option<&ReportModels>, o: Options, path: Option<&str>,
    progress: &mut dyn FnMut(usize, usize) -> bool,
) -> Result<Printed, String> {
    if pages.is_empty() {
        return Err("No page matches.".into());
    }
    // A headless print draws in the sheet font too (a no-op once installed).
    crate::styles::draw::register_sheet_font();
    let (scratch, sheets) = build_in(doc, pages, fan, tol, reports, o.pending, !o.mono)?;
    let mut done = Printed { sheets: sheets.len(), ..Printed::default() };
    // Nobody prints a pending book by mistake.
    done.notes.extend(ecad_electrical::releases::revisions::pending_note(doc, o.pending));
    let generated: BTreeSet<PageId> = pages.iter().copied().filter(|p| ecad_electrical::pages::generated::kind_of(doc, *p).is_some()).collect();
    done.generated = sheets.iter().filter(|s| generated.contains(&s.page)).count();
    done.flagged = ecad_electrical::pages::generated::flagged(doc, &generated, reports);
    let mut noted = BTreeSet::new();
    for s in sheets.iter().filter(|s| !s.exact) {
        if noted.insert(s.page) {
            let spec = ecad_model::document::sheet::sheet_of(doc, s.page);
            done.notes.push(format!("Page {} ({} x {}) is printed fitted on {}.", book::page_number(doc, s.page), spec.width, spec.height, s.paper));
        }
    }
    let mut written: Vec<(String, Vec<u8>, usize)> = Vec::new();
    let mut count = 0;
    for (i, vol) in volumes(&sheets).into_iter().enumerate() {
        count += vol.len();
        if !progress(count, sheets.len()) {
            return Err("Printing stopped; nothing written.".into());
        }
        let bytes = plot(&scratch, vol, o)?;
        match path {
            Some(p) => written.push((volume_path(p, i), bytes, vol.len())),
            None => done.files.push((None, vol.len(), bytes.len(), Some(bytes))),
        }
    }
    for (p, bytes, n) in written {
        write_file(&p, &bytes)?;
        done.files.push((Some(p), n, bytes.len(), None));
    }
    Ok(done)
}

/// Write `bytes` to `path` atomically (folders made as needed).
///
/// # Errors
/// The file system's error, in words.
pub fn write_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    cadcraft_engine::atomic_write::write_atomic_making_folders(std::path::Path::new(path), bytes).map_err(|e| format!("{path}: {e}"))
}

/// The dialog's notes for printing `scope` of `p`: a continuation sheet's number
/// chooses its whole page, and the pending-revision note. Reads only.
pub fn notes(p: &mut crate::Project, scope: &Scope) -> Vec<String> {
    if ecad_electrical::pages::generated::has_bom_page(p.ledger.document()) {
        p.reports();
    }
    p.refresh_pending();
    let doc = p.ledger.document();
    let mut out = resolve_with(doc, scope, p.kept_reports()).map(|r| r.notes).unwrap_or_default();
    out.extend(ecad_electrical::releases::revisions::pending_note(doc, p.frame.pending));
    out
}

/// What printing `scope` of `p` would give, for the dialog's live counts: sheets, numbers that
/// match no page, `?` values on generated pages. Reads pages only (and the kept report models).
pub fn count(p: &mut crate::Project, scope: &Scope) -> (usize, Vec<String>, usize) {
    if ecad_electrical::pages::generated::has_bom_page(p.ledger.document()) {
        p.reports();
    }
    let doc = p.ledger.document();
    let Ok(r) = resolve_with(doc, scope, p.kept_reports()) else { return (0, Vec::new(), 0) };
    let sheets = r.pages.iter().map(|pg| ecad_electrical::pages::generated::sheets_of(doc, *pg, p.kept_reports())).sum();
    let generated: BTreeSet<PageId> = r.pages.iter().copied().filter(|pg| ecad_electrical::pages::generated::kind_of(doc, *pg).is_some()).collect();
    (sheets, r.unknown, ecad_electrical::pages::generated::flagged(doc, &generated, p.kept_reports()))
}

/// Install CADCraft's file hooks (reading, writing and the PDF plotter) for an ECAD app, as
/// CADCraft's own apps do. Call once at start; later calls do nothing.
pub fn install_io() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: Some(|d, space, opts| cadcraft_io::plotter::plot(d, space, opts).map_err(|e| e.to_string())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn papers_are_the_smallest_iso_a_that_fits() {
        assert_eq!(paper_for(420.0, 297.0), ("A3", true, true));
        assert_eq!(paper_for(500.0, 300.0), ("A2", false, true));
        assert_eq!(paper_for(594.0, 420.0), ("A2", true, true));
    }

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b""), "");
    }

    #[test]
    fn volume_names_count_from_two() {
        assert_eq!(volume_path("demo.pdf", 0), "demo.pdf");
        assert!(volume_path("demo.pdf", 1).ends_with("demo-2.pdf"));
    }
}
