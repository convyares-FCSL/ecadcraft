//! Generated pages (brief 024, design 024 §5): a page whose content ECAD draws from the project
//! (cover, contents, revision list, BOM by location; brief 028 adds the terminal plan and the
//! cable schedule). It is a page like any other for numbers,
//! folders and the book; its kind is the page property [`KEY`] (design 024 FP1), set only when it
//! is made. Nothing drawn on it is stored.
//!
//! # Semantics
//! - A generated page has one or more **sheets**: a table that does not fit continues on the next
//!   sheet of the same page, header repeated ([`ROWS_PER_SHEET`] rows per A3 sheet). Sheets are
//!   drawn side by side, sheet *i* at x = *i* × (paper width + [`SHEET_GAP`]).
//! - Each sheet has the page's frame and title block: the page title gets ` (2/3)`, Next shows the
//!   same number on all but the last sheet, and Total counts sheets in the whole book (owner
//!   answer Q4).
//! - Content lies on the reserved, locked layer [`LAYER`]; it is mapped like the frame, so
//!   selecting it selects nothing and editing it is refused.
//!
//! # Update map (design 024 §9)
//! The project keeps the active page's parts with its frame. They are worked out again after an
//! op of the frame's map ([`crate::frame::touches`]), after any op [`touches`] names for the
//! active generated page's kind, and when the report models' BOM changes while a BOM page is shown
//! or the book's sheet count moves (`Project::update_outputs`). A terminal plan or cable schedule
//! page (brief 028) is redrawn the same way when the strip or cable rows changed while it is shown. Contents reads the book order
//! (O(pages)) only when it is drawn.

use std::collections::BTreeSet;

use cadcraft_engine::doc::{EntityKind, HAlign, Text, VAlign};
use cadcraft_engine::geom::Vec3;
use ecad_model::book::{self, Book};
use ecad_model::sheet::{MARGIN_MM, TITLE_BLOCK_MM, TOP_MARGIN_MM, unit_scale};
use ecad_model::{Document, ObjectRef, Op, PageId, Primitive, Pt};

use crate::convert::kind_for_primitive;
use crate::frame::{FrameInput, LABEL_ACI, Part, THIN_WEIGHT};
use crate::outputs::{Group, Outputs};

/// The page property that names a generated page's kind.
pub const KEY: &str = "generated";
/// Brief 028 (FP3): the strip a `terminals` page shows (a component ID in hex); absent: every strip.
pub const STRIP_KEY: &str = "strip";
/// The reserved layer of generated content (`ecad.generated`), locked.
pub const LAYER: &str = "ECAD-GENERATED";
/// Rows per A3 sheet.
pub const ROWS_PER_SHEET: usize = crate::outputs::ROWS_PER_SHEET;
/// Gap between the sheets of one page, in millimetres.
pub const SHEET_GAP: f64 = 50.0;
/// `ecad.table-header` and `ecad.table-row`.
pub const HEADER_MM: f64 = 7.0;
pub const ROW_MM: f64 = 5.0;
/// The column widths of design 024 §5.2 are for an A3 drawing area this wide.
const A3_AREA: f64 = 390.0;
/// `ecad.flagged` (`ecad.check-warning`, ACI 2): `?` values on screen.
pub const FLAGGED_ACI: u8 = 2;
/// `ecad.flagged-print` (ACI 7): a missing value's `?` and box when printed (design 033 §3.1).
pub const FLAGGED_PRINT_ACI: u8 = 7;
/// The text of a missing value.
pub const MISSING: &str = "?";
/// `ecad.missing-box`: the box round a missing value is inset this far into its cell (mm).
pub const MISSING_INSET_MM: f64 = 0.5;
/// Refusal of `pageprops` changing the kind.
pub const KIND_FIXED: &str = "A page's kind is set when it is made; add a new page instead.";

/// What a generated page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GenKind {
    Cover,
    Contents,
    Revisions,
    BomLocation,
    /// Brief 028: the terminal plan of every strip, or of the page's `strip`.
    Terminals,
    /// Brief 028: the cable schedule.
    Cables,
}

impl GenKind {
    pub const ALL: [GenKind; 6] = [GenKind::Cover, GenKind::Contents, GenKind::Revisions, GenKind::BomLocation, GenKind::Terminals, GenKind::Cables];

    /// Whether its content comes from the report models (built for it on first show).
    pub fn needs_outputs(self) -> bool {
        matches!(self, GenKind::BomLocation | GenKind::Terminals | GenKind::Cables)
    }

    /// The stored value.
    pub fn value(self) -> &'static str {
        match self {
            GenKind::Cover => "cover",
            GenKind::Contents => "contents",
            GenKind::Revisions => "revisions",
            GenKind::BomLocation => "bom-location",
            GenKind::Terminals => "terminals",
            GenKind::Cables => "cables",
        }
    }

    /// The kind a stored value names.
    pub fn parse(t: &str) -> Option<GenKind> {
        GenKind::ALL.into_iter().find(|k| k.value() == t.trim())
    }

    /// The default title.
    pub fn title(self) -> &'static str {
        match self {
            GenKind::Cover => "Cover",
            GenKind::Contents => "Contents",
            GenKind::Revisions => "Revision list",
            GenKind::BomLocation => "BOM by location",
            GenKind::Terminals => "Terminal plan",
            GenKind::Cables => "Cable schedule",
        }
    }
}

/// The kind of `page`, if it is generated.
pub fn kind_of(doc: &Document, page: PageId) -> Option<GenKind> {
    doc.pages.get(&page).and_then(|p| p.properties.get(KEY)).and_then(|v| GenKind::parse(v))
}

/// Whether the project has a page drawn from the report models (BOM, terminal plan, cable
/// schedule): its sheet count needs them.
pub fn has_bom_page(doc: &Document) -> bool {
    doc.pages.values().any(|p| p.properties.get(KEY).and_then(|v| GenKind::parse(v)).is_some_and(GenKind::needs_outputs))
}

/// The strip a `terminals` page shows, if it names one.
pub fn strip_of(doc: &Document, page: PageId) -> Option<ecad_model::ComponentId> {
    doc.pages.get(&page)?.properties.get(STRIP_KEY).and_then(|v| ecad_model::ComponentId::parse(v.trim()))
}

/// The strips a `terminals` page shows, in tag order.
fn strips_shown<'o>(doc: &Document, page: PageId, o: &'o Outputs) -> Vec<&'o crate::strips::StripData> {
    match strip_of(doc, page) {
        Some(c) => o.strips.get(c).into_iter().collect(),
        None => o.sorted_strips(),
    }
}

/// How many sheets a terminal plan needs: each strip starts a sheet, its heading repeated on
/// each sheet it runs onto (O(strips)).
fn terminal_sheets(doc: &Document, page: PageId, o: &Outputs) -> usize {
    // Kept per strip and in total (review of PR #55, finding 1): no walk of the rows.
    match strip_of(doc, page) {
        Some(c) => o.strips.get(c).map_or(1, |s| s.sheets),
        None => o.strips.sheets_total.max(1),
    }
}

/// How many sheets the BOM-by-location table needs.
pub fn bom_sheets(o: &Outputs) -> usize {
    o.bom_sheets()
}

/// The sheets of a table whose row `i` is a heading when `heads[i]` is `Some`: (first row, end,
/// the heading repeated as `(continued)` at the top). A continued heading takes a row of the
/// sheet, so every sheet holds at most [`ROWS_PER_SHEET`] lines (tester 024, bug 4).
pub fn paginate(heads: &[Option<String>]) -> Vec<(usize, usize, Option<String>)> {
    paginate_with(heads, false)
}

/// [`paginate`], with every heading after a sheet's first row starting a new sheet when
/// `break_on_heading` (the terminal plan: each strip from a new sheet).
pub fn paginate_with(heads: &[Option<String>], break_on_heading: bool) -> Vec<(usize, usize, Option<String>)> {
    let n = heads.len();
    let mut out = Vec::new();
    let mut start = 0;
    let mut current: Option<String> = None;
    loop {
        let cont = if start > 0 && start < n && heads.get(start).is_some_and(Option::is_none) { current.clone() } else { None };
        let cap = ROWS_PER_SHEET - usize::from(cont.is_some());
        let mut end = (start + cap).min(n);
        if break_on_heading && let Some(k) = heads.get(start + 1..end).and_then(|h| h.iter().position(Option::is_some)) {
            end = start + 1 + k;
        }
        for h in heads.get(start..end).unwrap_or_default().iter().flatten() {
            current = Some(h.clone());
        }
        out.push((start, end, cont));
        if end >= n {
            break;
        }
        start = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuation_sheets_are_numbered_in_sequence() {
        for (n, i, want) in [
            ("400", 0, "400"),
            ("400", 1, "401"),
            ("400", 2, "402"),
            ("0090", 1, "0091"),
            ("B10", 1, "B11"),
            ("099", 1, "100"),
            ("BOM", 1, "BOM2"),
            ("BOM", 2, "BOM3"),
            ("18446744073709551615", 1, "184467440737095516152"),
        ] {
            assert_eq!(sheet_number(n, i), want, "{n} sheet {i}");
        }
    }

    #[test]
    fn a_continued_heading_takes_a_row_of_its_sheet() {
        let mut heads = vec![Some("+CP1".to_string())];
        heads.extend(std::iter::repeat_n(None, 88));
        let sheets = paginate(&heads);
        assert_eq!(sheets.len(), 3);
        for (start, end, cont) in &sheets {
            assert!(end - start + usize::from(cont.is_some()) <= ROWS_PER_SHEET);
        }
        assert_eq!(sheets[1].2.as_deref(), Some("+CP1"));
        assert_eq!(paginate(&[]).len(), 1);
    }
}

/// How many sheets `page` prints on.
pub fn sheets(doc: &Document, page: PageId, outputs: Option<&Outputs>) -> usize {
    let rows = match kind_of(doc, page) {
        None | Some(GenKind::Cover) => return 1,
        Some(GenKind::Contents) => doc.page_order.len(),
        Some(GenKind::Revisions) => crate::revisions::issued(doc).len(),
        Some(GenKind::BomLocation) => return outputs.map_or(1, bom_sheets),
        Some(GenKind::Terminals) => return outputs.map_or(1, |o| terminal_sheets(doc, page, o)),
        Some(GenKind::Cables) => outputs.map_or(0, Outputs::cable_count),
    };
    rows.div_ceil(ROWS_PER_SHEET).max(1)
}

/// The number of sheet `i` (0-based) of a generated page numbered `number` (design 033 §5.1):
/// its trailing digits plus `i`, keeping their width (`400` → `401`, `0090` → `0091`, `B10` →
/// `B11`); without trailing digits, `N`, `N2`, `N3`… Sheet 0 is the number itself.
pub fn sheet_number(number: &str, i: usize) -> String {
    if i == 0 {
        return number.to_string();
    }
    let head = number.trim_end_matches(|c: char| c.is_ascii_digit());
    let digits = number.get(head.len()..).unwrap_or_default();
    if !digits.is_empty()
        && let Ok(n) = digits.parse::<u64>()
        && let Some(m) = u64::try_from(i).ok().and_then(|i| n.checked_add(i))
    {
        let width = digits.len();
        return format!("{head}{m:0width$}");
    }
    format!("{number}{}", i.saturating_add(1))
}

/// The numbers of every sheet of `page` (one for an ordinary page), its shown number first.
pub fn sheet_numbers(doc: &Document, book: &Book, page: PageId, outputs: Option<&Outputs>) -> Vec<String> {
    let n = book.number(doc, page);
    (0..sheets(doc, page, outputs)).map(|i| sheet_number(&n, i)).collect()
}

/// The number column of `page` where pages are listed (contents): `400`, or `400–402` for a
/// generated page of several sheets.
pub fn number_range(doc: &Document, book: &Book, page: PageId, outputs: Option<&Outputs>) -> String {
    let n = book.number(doc, page);
    match sheets(doc, page, outputs) {
        0 | 1 => n,
        k => format!("{n}\u{2013}{}", sheet_number(&n, k - 1)),
    }
}

/// The page whose continuation sheet (2nd or later) is numbered `number`, and that sheet's index:
/// continuation numbers count as taken (design 033 §5.3). O(generated pages × their sheets).
pub fn continuation_of(doc: &Document, book: &Book, number: &str, outputs: Option<&Outputs>) -> Option<(PageId, usize)> {
    book.order.iter().filter(|p| kind_of(doc, **p).is_some()).find_map(|p| {
        let n = book.number(doc, *p);
        (1..sheets(doc, *p, outputs)).find(|i| sheet_number(&n, *i).eq_ignore_ascii_case(number.trim())).map(|i| (*p, i))
    })
}

/// The refusal of typing `number` when it is sheet `i` of `page` (design 033 §5.3).
pub fn continuation_taken(doc: &Document, number: &str, page: PageId, i: usize) -> String {
    let title = doc.pages.get(&page).map(|p| p.title.as_str()).unwrap_or_default();
    let owner = ecad_model::book::page_number(doc, page);
    format!("Number {number} is sheet {} of page {owner} ({title}).", i.saturating_add(1))
}

/// The Checks rows of continuation sheets that took another page's number (design 033 §5.3): a
/// table grew, so the book still prints and the two sheets share the number until the user
/// renumbers. Each row is on the other page. O(pages + generated pages × their sheets).
pub fn continuation_checks(doc: &Document, outputs: Option<&Outputs>) -> Vec<crate::checks::Check> {
    use crate::checks::{Check, Fix, Kind, Object};
    let book = Book::of(doc);
    let mut by_number: std::collections::HashMap<String, PageId> = std::collections::HashMap::new();
    for p in &book.order {
        by_number.entry(book.number(doc, *p).to_ascii_lowercase()).or_insert(*p);
    }
    let mut out = Vec::new();
    for p in book.order.iter().filter(|p| kind_of(doc, **p).is_some()) {
        let n = sheets(doc, *p, outputs);
        if n < 2 {
            continue;
        }
        let number = book.number(doc, *p);
        let last = sheet_number(&number, n - 1);
        let title = doc.pages.get(p).map(|x| x.title.as_str()).unwrap_or_default();
        for i in 1..n {
            let taken = sheet_number(&number, i);
            if let Some(other) = by_number.get(&taken.to_ascii_lowercase()).filter(|o| *o != p) {
                out.push(Check {
                    kind: Kind::ContinuationClash,
                    page: Some(*other),
                    at: None,
                    object: Object::None,
                    which: format!("Page {number} ({title}) now runs to {last}, and page {taken} has that number."),
                    fix: Some(Fix::RenumberPage(*other)),
                });
            }
        }
    }
    out
}

/// How many sheets the whole book has (Total).
pub fn book_sheets(doc: &Document, outputs: Option<&Outputs>) -> usize {
    let extra: usize =
        doc.pages.iter().filter(|(_, p)| p.properties.contains_key(KEY)).map(|(id, _)| sheets(doc, *id, outputs).saturating_sub(1)).sum();
    doc.page_order.len() + extra
}

/// Whether `ops` can change what a generated page of `kind` shows (beyond the frame's own map
/// and the report models, which the project watches itself).
pub fn touches(ops: &[Op], kind: GenKind) -> bool {
    ops.iter().any(|op| match (kind, op) {
        (
            GenKind::Contents,
            Op::AddPage { .. } | Op::RemovePage { .. } | Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. },
        ) => true,
        (GenKind::Contents, Op::SetProperty { object: ObjectRef::Page(_), key, .. }) => key == book::NUMBER_KEY || key == book::FOLDER_KEY,
        (GenKind::Cover | GenKind::Revisions, Op::SetProperty { object: ObjectRef::Project, .. }) => true,
        (GenKind::Cover | GenKind::Revisions, Op::NameVersion { .. } | Op::UnnameVersion { .. }) => true,
        (GenKind::Cover, Op::AddPage { .. } | Op::RemovePage { .. }) => true,
        _ => false,
    })
}

/// The refusal of `ops` when they would draw on a generated page (design 024 §5.3), before
/// anything is written.
pub fn refusal(doc: &Document, ops: &[Op]) -> Option<String> {
    ops.iter()
        .filter_map(|op| match op {
            Op::AddItem { item, .. } => Some(item.page),
            Op::LinkRepresentation { rep, .. } => Some(rep.page),
            Op::ModifyItem { after, .. } => Some(after.page),
            _ => None,
        })
        .find_map(|p| kind_of(doc, p).map(|k| refusal_words(doc, p, k)))
}

/// `Page 900 is generated (BOM by location). Draw on another page, or add one after it with +.`
pub fn refusal_words(doc: &Document, page: PageId, kind: GenKind) -> String {
    format!("Page {} is generated ({}). Draw on another page, or add one after it with +.", book::page_number(doc, page), kind.title())
}

/// Draws in paper millimetres, sheet origin at `x0`.
struct Draw {
    k: f64,
    x0: f64,
    out: Vec<Part>,
}

impl Draw {
    fn p(&self, x: f64, y: f64) -> Pt {
        Pt::new((x + self.x0) * self.k, y * self.k)
    }
    fn line(&mut self, a: (f64, f64), b: (f64, f64)) {
        let kind = kind_for_primitive(&Primitive::Line { a: self.p(a.0, a.1), b: self.p(b.0, b.1) });
        self.out.push(Part { kind, weight: Some(THIN_WEIGHT), aci: None, layer: LAYER });
    }
    /// A filled dot of `radius` millimetres (concentric circles, like the junction glyph).
    fn dot(&mut self, at: (f64, f64), radius: f64) {
        let center = self.p(at.0, at.1);
        for k in [1.0, 0.66, 0.33] {
            let kind = kind_for_primitive(&Primitive::Circle { center, radius: radius * k * self.k });
            self.out.push(Part { kind, weight: Some(THIN_WEIGHT), aci: None, layer: LAYER });
        }
    }
    /// A closed rectangle from `a` to `b` (paper millimetres), weight `weight`, colour `aci`.
    fn rect(&mut self, a: (f64, f64), b: (f64, f64), weight: u16, aci: Option<u8>) {
        let v = vec![self.p(a.0, a.1), self.p(b.0, a.1), self.p(b.0, b.1), self.p(a.0, b.1)];
        let kind = kind_for_primitive(&Primitive::Polyline { vertices: v, closed: true });
        self.out.push(Part { kind, weight: Some(weight), aci, layer: LAYER });
    }
    fn text(&mut self, at: (f64, f64), height: f64, value: &str, h: HAlign, aci: Option<u8>, room: Option<f64>) {
        if value.is_empty() {
            return;
        }
        let (shown, wf) = crate::frame::fit(value, height, room);
        let at = self.p(at.0, at.1);
        let pt = Vec3::new(at.x, at.y, 0.0);
        let kind = EntityKind::Text(Text {
            insert: pt,
            align_pt: Some(pt),
            height: height * self.k,
            value: shown,
            rotation: 0.0,
            width_factor: wf,
            oblique: 0.0,
            style: "Standard".into(),
            halign: h,
            valign: VAlign::Middle,
        });
        self.out.push(Part { kind, weight: None, aci, layer: LAYER });
    }
}

/// The missing-data mark (design 033 §3.1, `ecad.missing-box`): `?` in a thin box inset
/// [`MISSING_INSET_MM`] into the cell from `lo` to `hi` (model coordinates, `k` drawing units per
/// millimetre), the text `height` millimetres high. Both parts are [`FLAGGED_ACI`] on [`LAYER`]:
/// yellow on screen; Print Book prints them in [`FLAGGED_PRINT_ACI`].
pub fn missing_mark(lo: Pt, hi: Pt, k: f64, height: f64) -> Vec<Part> {
    if !(k.is_finite() && k > 0.0) || !lo.is_finite() || !hi.is_finite() {
        return Vec::new();
    }
    let mut d = Draw { k, x0: 0.0, out: Vec::new() };
    let (x0, y0, x1, y1) = (lo.x.min(hi.x) / k, lo.y.min(hi.y) / k, lo.x.max(hi.x) / k, lo.y.max(hi.y) / k);
    let inset = MISSING_INSET_MM.min((x1 - x0) / 4.0).min((y1 - y0) / 4.0);
    d.rect((x0 + inset, y0 + inset), (x1 - inset, y1 - inset), crate::look::WIRE_WEIGHT, Some(FLAGGED_ACI));
    d.text((x0 + inset + 1.0, (y0 + y1) / 2.0), height, MISSING, HAlign::Left, Some(FLAGGED_ACI), Some(x1 - x0 - 2.0 * inset - 1.0));
    d.out
}

/// The drawing area of a sheet in paper millimetres: left, bottom, right, top.
fn area(input: &FrameInput) -> (f64, f64, f64, f64) {
    let (w, h) = (f64::from(input.sheet.width), f64::from(input.sheet.height));
    (MARGIN_MM + 5.0, MARGIN_MM + TITLE_BLOCK_MM + 5.0, w - MARGIN_MM - 5.0, h - TOP_MARGIN_MM - 5.0)
}

/// A table row: cells, and whether it is a heading (BOM location blocks, strips).
#[derive(Debug, Clone, Default)]
struct TRow {
    cells: Vec<String>,
    heading: bool,
    /// Brief 028: the terminal plan's marks.
    marks: Marks,
}

/// What a terminal plan row draws beyond its text (design 028 §5.3).
#[derive(Debug, Clone, Default, PartialEq)]
struct Marks {
    /// Column of the terminal box (its label centred in a 6 × 4 mm box).
    terminal_box: Option<usize>,
    /// The bridge group whose dot this row carries, in the bridge column.
    bridge: Option<(usize, usize)>,
    /// The cable column and whether this row starts or continues a cable's bracket.
    cable: Option<(usize, bool)>,
}

/// What a generated page shows: (columns with A3 widths, rows).
fn table(doc: &Document, kind: GenKind, outputs: Option<&Outputs>) -> (Vec<(&'static str, f64)>, Vec<TRow>) {
    let row = |cells: Vec<String>| TRow { cells, ..TRow::default() };
    match kind {
        GenKind::Cover => (Vec::new(), Vec::new()),
        GenKind::Contents => {
            let b = Book::of(doc);
            let rows = b
                .order
                .iter()
                .map(|p| {
                    let folder = book::folder_of(doc, *p).filter(|f| doc.folders.contains_key(f));
                    let title = doc.pages.get(p).map(|x| x.title.clone()).unwrap_or_default();
                    row(vec![number_range(doc, &b, *p, outputs), title, crate::sheet_cmds::folder_path(doc, folder)])
                })
                .collect();
            (vec![("Page", 25.0), ("Title", 220.0), ("Folder", 145.0)], rows)
        }
        GenKind::Revisions => {
            let issued = crate::revisions::issued(doc);
            let rows = if issued.is_empty() {
                vec![row(vec![String::new(), String::new(), "No revisions issued yet.".into()])]
            } else {
                issued
                    .iter()
                    .map(|r| {
                        let mut c: Vec<String> = r.fields.to_vec();
                        c.push(r.issued.clone().unwrap_or_default());
                        row(c)
                    })
                    .collect()
            };
            let cols = vec![
                ("Rev", 15.0),
                ("Date", 25.0),
                ("Description", 200.0),
                ("Drawn", 30.0),
                ("Checked", 30.0),
                ("Approved", 30.0),
                ("Version", 60.0),
            ];
            (cols, rows)
        }
        GenKind::BomLocation => {
            let cols = vec![("Qty", 15.0), ("Tags", 70.0), ("Description", 140.0), ("Maker", 70.0), ("Part number", 95.0)];
            let Some(o) = outputs else { return (cols, Vec::new()) };
            let mut rows = Vec::new();
            let mut last: Option<Option<String>> = None;
            let q = |v: &Option<String>| v.clone().unwrap_or_else(|| "?".into());
            for r in o.bom(Group::Location) {
                if last.as_ref() != Some(&r.location) {
                    rows.push(TRow { cells: vec![q(&r.location)], heading: true, ..TRow::default() });
                    last = Some(r.location.clone());
                }
                rows.push(row(vec![r.qty.to_string(), r.tags.join(", "), q(&r.description), q(&r.maker), q(&r.part_number)]));
            }
            if rows.is_empty() {
                rows.push(row(vec![String::new(), "No devices yet.".into()]));
            }
            (cols, rows)
        }
        // Brief 028 (design 028 §5.3), drawn by `terminal_rows`.
        GenKind::Terminals => (TERMINAL_COLUMNS.to_vec(), Vec::new()),
        GenKind::Cables => {
            let cols = vec![
                ("Tag", 25.0),
                ("Type", 45.0),
                ("Cores", 15.0),
                ("Used", 15.0),
                ("Spare", 40.0),
                ("From", 85.0),
                ("To", 85.0),
                ("Length", 20.0),
                ("Designation", 60.0),
            ];
            let Some(o) = outputs else { return (cols, Vec::new()) };
            let q = |v: &Option<String>| v.clone().unwrap_or_else(|| "?".into());
            let ends = |l: &Option<String>, d: &[String]| {
                let mut v: Vec<String> = l.iter().cloned().collect();
                v.extend(d.iter().cloned());
                v.join(" ")
            };
            let mut rows: Vec<TRow> = o
                .cables()
                .into_iter()
                .map(|r| {
                    row(vec![
                        q(&r.tag),
                        q(&r.kind),
                        r.core_total().to_string(),
                        r.count.used().to_string(),
                        q(&r.count.spare_text()),
                        ends(&r.from_location, &r.from_devices),
                        ends(&r.to_location, &r.to_devices),
                        r.length.clone().unwrap_or_default(),
                        r.designation.clone().unwrap_or_default(),
                    ])
                })
                .collect();
            if rows.is_empty() {
                rows.push(row(vec![String::new(), "No cables yet.".into()]));
            }
            (cols, rows)
        }
    }
}

/// The terminal plan's columns with their A3 widths (design 028 §5.3).
const TERMINAL_COLUMNS: [(&str, f64); 9] = [
    ("Inside target", 70.0),
    ("Inside wire", 30.0),
    ("Terminal", 16.0),
    ("Bridge", 10.0),
    ("Outside wire", 30.0),
    ("Cable", 24.0),
    ("Core", 16.0),
    ("Outside target", 70.0),
    ("Place", 24.0),
];

/// The terminal plan's rows of `page`: per strip a heading, then one line per position and
/// conductor (a second conductor on one side on its own sub-row, labelled `3′`).
fn terminal_rows(doc: &Document, page: PageId, o: &Outputs) -> Vec<TRow> {
    let style = doc.settings.reference_style;
    let mut rows = Vec::new();
    for s in strips_shown(doc, page, o) {
        let mut head = format!("{}  {}  {}", s.tag, s.location.clone().unwrap_or_else(|| "?".into()), s.counts_text());
        if let Some(pn) = &s.part_number {
            head.push_str(&format!("  (part: {pn})"));
        }
        // Brief 030 (design 030 §7.1): the accessory totals.
        let acc = s.acc.summary();
        if !acc.is_empty() {
            head.push_str(&format!(" \u{b7} {acc}"));
        }
        rows.push(TRow { cells: vec![head], heading: true, ..TRow::default() });
        let mut last_cable: Option<ecad_model::CableId> = None;
        for item in s.plan(true) {
            let (ri, r) = match item {
                crate::strips::PlanItem::Term(i, r) => (i, r),
                // Brief 030 (design 030 §7.3): a gap row is a full row, the word in Inside target,
                // a mark across Terminal, the part in Place.
                crate::strips::PlanItem::Gap { gap, acc } => {
                    let mut cells = vec![String::new(); TERMINAL_COLUMNS.len()];
                    let words = if acc == crate::accessories::Acc::StripLabel { format!("strip label {}", s.tag) } else { acc.words().to_string() };
                    let mark = match acc {
                        crate::accessories::Acc::EndStop => "||",
                        crate::accessories::Acc::EndPlate => "|",
                        crate::accessories::Acc::Partition => "==",
                        _ => "[ ]",
                    };
                    let part = s.acc.gap_part(acc, crate::outputs::gap_pe(s, gap)).cloned().unwrap_or_else(|| "?".into());
                    for (i, v) in [(0, words), (2, mark.to_string()), (8, part)] {
                        if let Some(c) = cells.get_mut(i) {
                            *c = v;
                        }
                    }
                    rows.push(TRow { cells, heading: false, marks: Marks::default() });
                    last_cable = None;
                    continue;
                }
            };
            let marker = if s.acc.markers.contains(&r.label) { " []" } else { "" };
            let links: String = {
                let at = s.links_at(ri);
                let lanes = at.iter().map(|(l, _)| l.lane + 1).max().unwrap_or(0);
                let mut m = vec![' '; lanes];
                for (l, on) in &at {
                    if let Some(x) = m.get_mut(l.lane) {
                        *x = if *on { 'o' } else { ':' };
                    }
                }
                m.into_iter().collect::<String>().trim_end().to_string()
            };
            if r.spare() {
                let mut cells = vec![String::new(); TERMINAL_COLUMNS.len()];
                if let Some(c) = cells.get_mut(0) {
                    *c = "spare".into();
                }
                if let Some(c) = cells.get_mut(2) {
                    *c = if r.pe { format!("{} PE{marker}", r.label) } else { format!("{}{marker}", r.label) };
                }
                if let Some(c) = cells.get_mut(3) {
                    *c = links.clone();
                }
                rows.push(TRow { cells, heading: false, marks: Marks { terminal_box: Some(2), ..Marks::default() } });
                last_cable = None;
                continue;
            }
            let lines = r.inside.len().max(r.outside.len()).max(1);
            for i in 0..lines {
                let inside = r.inside.get(i);
                let outside = r.outside.get(i);
                let q = |v: Option<&String>| v.cloned().unwrap_or_else(|| "?".into());
                let label = if i == 0 { format!("{}{marker}", r.label) } else { format!("{}\u{2032}", r.label) };
                let label = if r.pe { format!("{label} PE") } else { label };
                let cable = outside.and_then(|c| c.cable.as_ref());
                let starts = cable.is_some_and(|(k, _)| last_cable != Some(*k));
                last_cable = cable.map(|(k, _)| *k);
                let cells = vec![
                    inside.map_or_else(String::new, |c| q(c.target.as_ref())),
                    inside.map_or_else(String::new, |c| q(c.wire.as_ref())),
                    label,
                    if i == 0 && r.bridge_group.is_none() { links.clone() } else { String::new() },
                    outside.map_or_else(String::new, |c| q(c.wire.as_ref())),
                    match cable {
                        Some((_, t)) if starts => t.clone().unwrap_or_else(|| "?".into()),
                        _ => String::new(),
                    },
                    match (cable, outside) {
                        (Some(_), Some(c)) => q(c.core.as_ref()),
                        _ => String::new(),
                    },
                    outside.map_or_else(String::new, |c| q(c.target.as_ref())),
                    if i == 0 { r.place.as_ref().map(|p| p.text(style)).unwrap_or_default() } else { String::new() },
                ];
                let marks = Marks {
                    terminal_box: (i == 0).then_some(2),
                    bridge: r.bridge_group.filter(|_| i == 0).map(|g| (3, g)),
                    cable: cable.map(|_| (5, starts)),
                };
                rows.push(TRow { cells, heading: false, marks });
            }
        }
    }
    if rows.is_empty() {
        rows.push(TRow { cells: vec![String::new(), String::new(), "No terminal strips yet.".into()], ..TRow::default() });
    }
    rows
}

/// The cover's lines: (text, height).
fn cover_lines(doc: &Document, total: usize) -> Vec<(String, f64)> {
    let prop = |k: &str| doc.properties.get(k).cloned().filter(|v| !v.trim().is_empty());
    let mut out = Vec::new();
    for (k, h) in [(crate::frame::TITLE, 10.0), (crate::frame::DRAWING_NUMBER, 7.0), (crate::frame::CUSTOMER, 5.0), (crate::frame::STATUS, 5.0)] {
        if let Some(v) = prop(k) {
            out.push((v, h));
        }
    }
    let rev = match crate::revisions::issued(doc).first() {
        Some(r) => {
            let date = r.get("date");
            if date.is_empty() { format!("Revision {}", r.rev()) } else { format!("Revision {}, {date}", r.rev()) }
        }
        None => "Not issued".into(),
    };
    out.push((rev, 5.0));
    out.push((if total == 1 { "1 sheet".into() } else { format!("{total} sheets") }, 3.5));
    out
}

/// The content of sheet `i` of a generated page of `kind`, with its frame `input` (paper
/// millimetres, sheet origin at x = `x0`).
#[allow(clippy::too_many_arguments)]
fn content(doc: &Document, page: PageId, kind: GenKind, input: &FrameInput, i: usize, x0: f64, outputs: Option<&Outputs>, total: usize) -> Vec<Part> {
    let (l, b, r, t) = area(input);
    let mut d = Draw { k: unit_scale(input.unit), x0, out: Vec::new() };
    if kind == GenKind::Cover {
        let lines = cover_lines(doc, total);
        let gap = 6.0;
        let height: f64 = lines.iter().map(|(_, h)| h + gap).sum();
        let mut y = (b + t) / 2.0 + height / 2.0;
        for (text, h) in lines {
            y -= h / 2.0 + gap / 2.0;
            d.text(((l + r) / 2.0, y), h, &text, HAlign::Center, None, Some(r - l));
            y -= h / 2.0 + gap / 2.0;
        }
        return d.out;
    }
    let (cols, mut rows) = table(doc, kind, outputs);
    if kind == GenKind::Terminals
        && let Some(o) = outputs
    {
        rows = terminal_rows(doc, page, o);
    }
    let scale = (r - l) / A3_AREA;
    let widths: Vec<f64> = cols.iter().map(|(_, w)| w * scale).collect();
    let mut y = t;
    // Header.
    let mut x = l;
    for ((name, _), w) in cols.iter().zip(&widths) {
        d.text((x + 1.0, y - HEADER_MM / 2.0), 2.5, name, HAlign::Left, Some(LABEL_ACI), Some(w - 2.0));
        x += w;
    }
    y -= HEADER_MM;
    d.line((l, y), (r, y));
    // The sheets as the sheet count makes them; a sheet starting inside a location block repeats it
    // as `(continued)`, which takes one of its rows.
    let heads: Vec<Option<String>> = rows.iter().map(|r| r.heading.then(|| r.cells.first().cloned().unwrap_or_default())).collect();
    let (start, end, prev_heading) = paginate_with(&heads, kind == GenKind::Terminals).get(i).cloned().unwrap_or((0, 0, None));
    let page_rows: Vec<&TRow> = rows.get(start..end).unwrap_or_default().iter().collect();
    if let Some(h) = prev_heading {
        d.text((l + 1.0, y - ROW_MM / 2.0), 3.5, &format!("{h} (continued)"), HAlign::Left, None, Some(r - l - 2.0));
        y -= ROW_MM;
        d.line((l, y), (r, y));
    }
    let left_of = |c: usize| l + widths.iter().take(c).sum::<f64>();
    let mut last_dot: Option<(usize, f64)> = None;
    for row in page_rows {
        if row.heading {
            d.text((l + 1.0, y - ROW_MM / 2.0), 3.5, row.cells.first().map_or("", String::as_str), HAlign::Left, None, Some(r - l - 2.0));
            last_dot = None;
        } else {
            let mut x = l;
            let mid = y - ROW_MM / 2.0;
            for (c, (cell, w)) in row.cells.iter().zip(&widths).enumerate() {
                let aci = (cell == MISSING).then_some(FLAGGED_ACI);
                if cell == MISSING && row.marks.terminal_box != Some(c) {
                    d.out.extend(missing_mark(d.p(x, y - ROW_MM), d.p(x + w, y), d.k, 2.5));
                } else if row.marks.terminal_box == Some(c) {
                    let cx = x + w / 2.0;
                    d.line((cx - 3.0, mid - 2.0), (cx + 3.0, mid - 2.0));
                    d.line((cx + 3.0, mid - 2.0), (cx + 3.0, mid + 2.0));
                    d.line((cx + 3.0, mid + 2.0), (cx - 3.0, mid + 2.0));
                    d.line((cx - 3.0, mid + 2.0), (cx - 3.0, mid - 2.0));
                    d.text((cx, mid), 2.5, cell, HAlign::Center, aci, Some(*w - 2.0));
                } else {
                    d.text((x + 1.0, mid), 2.5, cell, HAlign::Left, aci, Some(w - 2.0));
                }
                x += w;
            }
            if let Some((c, group)) = row.marks.bridge {
                let cx = left_of(c) + widths.get(c).copied().unwrap_or(0.0) / 2.0;
                d.dot((cx, mid), 0.5);
                if let Some((g, y0)) = last_dot
                    && g == group
                {
                    d.line((cx, y0), (cx, mid));
                }
                last_dot = Some((group, mid));
            }
            if let Some((c, starts)) = row.marks.cable {
                let x0 = left_of(c) + 0.5;
                let top = if starts { mid } else { y };
                d.line((x0, top), (x0, y - ROW_MM));
            }
        }
        y -= ROW_MM;
        if y < b {
            break;
        }
        d.line((l, y), (r, y));
    }
    d.out
}

/// Every frame and content part of `page`: one sheet for an ordinary page, every sheet of a
/// generated one side by side. `input` is the page's frame input with Total already counting
/// sheets.
pub fn page_parts(doc: &Document, page: PageId, input: &FrameInput, outputs: Option<&Outputs>, total: usize) -> Vec<Part> {
    let Some(kind) = kind_of(doc, page) else { return crate::frame::parts(input) };
    let n = sheets(doc, page, outputs);
    let width = f64::from(input.sheet.width) + SHEET_GAP;
    let mut out = Vec::new();
    for i in 0..n {
        let mut inp = input.clone();
        if n > 1 {
            inp.page_title = format!("{} ({}/{n})", input.page_title, i + 1);
            // Design 033 §5.2: each sheet its own number; Next is the next sheet's.
            inp.page = sheet_number(&input.page, i);
            if i + 1 < n {
                inp.next = sheet_number(&input.page, i + 1);
            }
        }
        let x0 = width * i as f64;
        let mut frame = crate::frame::parts(&inp);
        if x0 != 0.0 {
            let m = cadcraft_engine::geom::Mat3::translate(cadcraft_engine::geom::Vec2::new(x0 * unit_scale(input.unit), 0.0));
            for p in &mut frame {
                p.kind.transform(&m);
            }
        }
        out.extend(frame);
        out.extend(content(doc, page, kind, &inp, i, x0, outputs, total));
    }
    out
}

/// The sheet windows of `page` in model coordinates: (bottom-left, top-right) per sheet.
pub fn sheet_windows(doc: &Document, page: PageId, outputs: Option<&Outputs>) -> Vec<(Pt, Pt)> {
    let spec = ecad_model::sheet::sheet_of(doc, page);
    let k = unit_scale(doc.settings.length_unit);
    let (w, h) = (f64::from(spec.width), f64::from(spec.height));
    (0..sheets(doc, page, outputs))
        .map(|i| {
            let x0 = (w + SHEET_GAP) * i as f64;
            (Pt::new(x0 * k, 0.0), Pt::new((x0 + w) * k, h * k))
        })
        .collect()
}

/// How many generated values in `pages` are `?` (Print Book's warning line).
pub fn flagged(doc: &Document, pages: &BTreeSet<PageId>, outputs: Option<&Outputs>) -> usize {
    pages
        .iter()
        .map(|p| match kind_of(doc, *p) {
            Some(GenKind::BomLocation) => outputs.map_or(0, |o| o.bom_table(Group::Location).flagged()),
            Some(GenKind::Cables) => outputs.map_or(0, |o| o.cable_table().flagged()),
            _ => 0,
        })
        .sum()
}

/// The frame input of `page` with Total counting sheets, and that total.
pub fn input_of(doc: &Document, page: PageId, outputs: Option<&Outputs>) -> (FrameInput, usize) {
    let mut input = FrameInput::of(doc, page, &Book::of(doc));
    let total = book_sheets(doc, outputs);
    input.total = total.to_string();
    (input, total)
}

/// Every frame and content part of `page`, building the report models only when the project has a
/// BOM page and none are given (debugger, verification, tests).
pub fn parts_of(doc: &Document, page: PageId, outputs: Option<&Outputs>) -> Vec<Part> {
    let built;
    let outputs = match outputs {
        Some(o) => Some(o),
        None if has_bom_page(doc) => {
            built = Outputs::build(doc);
            Some(&built)
        }
        None => None,
    };
    let (input, total) = input_of(doc, page, outputs);
    page_parts(doc, page, &input, outputs, total)
}
