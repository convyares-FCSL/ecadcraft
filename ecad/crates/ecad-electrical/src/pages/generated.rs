//! Generated pages (brief 024, design 024 §5): a page whose content ECAD draws from the project
//! (cover, contents, revision list, BOM by location; brief 028 adds the terminal plan and the
//! cable schedule). It is a page like any other for numbers,
//! folders and the book; its kind is [`ecad_model::Page::kind`] (design 072 §5.1), set only when it
//! is made. Nothing drawn on it is stored.
//!
//! # Semantics
//! - A generated page has one or more **sheets**: a table that does not fit continues on the next
//!   sheet of the same page, header repeated ([`rows_per_sheet`] rows per A3 sheet). Sheets are
//!   drawn side by side, sheet *i* at x = *i* × (paper width + [`SHEET_GAP`]).
//! - Each sheet has the page's frame and title block: the page title gets ` (2/3)`, Next shows the
//!   same number on all but the last sheet, and Total counts sheets in the whole book (owner
//!   answer Q4).
//! - Content lies on the reserved, locked layer [`LAYER`]; it is mapped like the frame, so
//!   selecting it selects nothing and editing it is refused.
//!
//! # Update map (design 024 §9)
//! The project keeps the active page's parts with its frame. They are worked out again after an
//! op of the frame's map ([`crate::pages::frames::touches`]), after any op [`touches`] names for the
//! active generated page's kind, and when the report models' BOM changes while a BOM page is shown
//! or the book's sheet count moves (`derived::ReportsView`). A terminal plan or cable schedule
//! page (brief 028) is redrawn the same way when the strip or cable rows changed while it is shown. Contents reads the book order
//! (O(pages)) only when it is drawn.

use std::collections::BTreeSet;

use ecad_model::document::book::{self, Book};
use ecad_model::document::sheet::unit_scale;
use ecad_model::{Document, ObjectRef, Op, PageId, Pt};

use crate::reports::Group;
use crate::reports::kept::ReportModels;

/// The reserved layer of generated content (`ecad.generated`), locked.
pub const LAYER: &str = "ECAD-GENERATED";
/// Gap between the sheets of one page, in millimetres.
pub const SHEET_GAP: f64 = 50.0;
/// The column widths of design 024 §5.2 are for an A3 drawing area this wide.
pub const A3_AREA: f64 = 390.0;
/// The height of that A3 drawing area ([`area`] of an A3 sheet in the house frame): what the rows
/// per sheet are computed from (brief 066c).
pub const A3_TABLE_MM: f64 = 230.0;

/// `ecad.table-row`: a report table's row height, millimetres (the `[layout]` value
/// `page.report.row_height` in `pages.toml`, data since brief 066c).
pub fn row_mm() -> f64 {
    crate::defaults::data::layout("page.report.row_height")
}

/// `ecad.table-header`: a report table's header height, millimetres (`page.report.header_height`).
pub fn header_mm() -> f64 {
    crate::defaults::data::layout("page.report.header_height")
}

/// Rows per A3 sheet of a generated table (design 024 §5.2): as many rows as fit under the header
/// in [`A3_TABLE_MM`] (44 at 5 mm rows and a 7 mm header), at least 2.
pub fn rows_per_sheet() -> usize {
    static ROWS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *ROWS.get_or_init(|| {
        let n = ((A3_TABLE_MM - header_mm()) / row_mm()).floor();
        if n.is_finite() && n >= 2.0 { n as usize } else { 2 }
    })
}
/// `ecad.flagged` (`ecad.check-warning`, ACI 2): `?` values on screen.
pub const FLAGGED_ACI: u8 = 2;
/// `ecad.flagged-print` (ACI 7): a missing value's `?` and box when printed (design 033 §3.1).
pub const FLAGGED_PRINT_ACI: u8 = 7;
/// The text of a missing value.
pub const MISSING: &str = "?";
/// Refusal of `pageprops` changing the kind.
pub const KIND_FIXED: &str = "A page's kind is set when it is made; add a new page instead.";

use ecad_model::GenKind;

/// The kind of `page`, if it is generated.
pub fn kind_of(doc: &Document, page: PageId) -> Option<GenKind> {
    doc.pages.get(&page).and_then(|p| p.kind.generated_kind())
}

/// Whether the project has a page drawn from the report models (BOM, terminal plan, cable
/// schedule): its sheet count needs them.
pub fn has_bom_page(doc: &Document) -> bool {
    doc.pages.values().any(|p| p.kind.generated_kind().is_some_and(GenKind::needs_reports))
}

/// The strip a `connection_points` page shows, if it names one.
pub fn strip_of(doc: &Document, page: PageId) -> Option<ecad_model::DeviceId> {
    doc.pages.get(&page)?.kind.strip()
}

/// The strips a `connection_points` page shows, in tag order.
fn strips_shown<'o>(doc: &Document, page: PageId, o: &'o ReportModels) -> Vec<&'o crate::terminal_strips::strip_model::StripData> {
    match strip_of(doc, page) {
        Some(c) => o.strips.get(c).into_iter().collect(),
        None => o.sorted_strips(),
    }
}

/// How many sheets a terminal plan needs: each strip starts a sheet, its heading repeated on
/// each sheet it runs onto (O(strips)).
fn terminal_block_sheets(doc: &Document, page: PageId, o: &ReportModels) -> usize {
    // Kept per strip and in total (review of PR #55, finding 1): no walk of the rows.
    match strip_of(doc, page) {
        Some(c) => o.strips.get(c).map_or(1, |s| s.sheets),
        None => o.strips.sheets_total.max(1),
    }
}

/// How many sheets the BOM-by-location table needs.
pub fn bom_sheets(o: &ReportModels) -> usize {
    o.bom_sheets()
}

/// The sheets of a table whose row `i` is a heading when `heads[i]` is `Some`: (first row, end,
/// the heading repeated as `(continued)` at the top). A continued heading takes a row of the
/// sheet, so every sheet holds at most [`rows_per_sheet`] lines (tester 024, bug 4).
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
        let cap = rows_per_sheet() - usize::from(cont.is_some());
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
            assert!(end - start + usize::from(cont.is_some()) <= rows_per_sheet());
        }
        assert_eq!(sheets[1].2.as_deref(), Some("+CP1"));
        assert_eq!(paginate(&[]).len(), 1);
    }
}

/// How many sheets `page` prints on.
pub fn sheets_of(doc: &Document, page: PageId, reports: Option<&ReportModels>) -> usize {
    let rows = match kind_of(doc, page) {
        None | Some(GenKind::Cover) => return 1,
        Some(GenKind::Contents) => doc.page_order.len(),
        Some(GenKind::Revisions) => crate::releases::revisions::issued(doc).len(),
        Some(GenKind::BomLocation) => return reports.map_or(1, bom_sheets),
        // Brief 046: a graphic page counts its strips' kept graphic sheets.
        Some(GenKind::TerminalBlocks) if crate::terminal_strips::pages::style_of(doc, page) == crate::terminal_strips::pages::Style::Graphic => {
            return reports.map_or(1, |o| crate::terminal_strips::pages::sheets(doc, page, o));
        }
        Some(GenKind::TerminalBlocks) => return reports.map_or(1, |o| terminal_block_sheets(doc, page, o)),
        Some(GenKind::Cables) => reports.map_or(0, ReportModels::cable_count),
        // Brief 051 (design 051 §4.1): cards packed on sheets, from the kept row counts.
        Some(GenKind::IoList) => return reports.map_or(1, |o| o.plc.iolist_sheets(rows_per_sheet())),
        // Brief 051 (design 051 §4.2): racks packed on sheets, from the kept slot counts.
        Some(GenKind::PlcRack) => return reports.map_or(1, |o| crate::plc::sheets::rack_sheets(doc, page, &o.plc)),
    };
    rows.div_ceil(rows_per_sheet()).max(1)
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
pub fn sheet_numbers(doc: &Document, book: &Book, page: PageId, reports: Option<&ReportModels>) -> Vec<String> {
    let n = book.number(doc, page);
    (0..sheets_of(doc, page, reports)).map(|i| sheet_number(&n, i)).collect()
}

/// The number column of `page` where pages are listed (contents): `400`, or `400–402` for a
/// generated page of several sheets.
pub fn number_range(doc: &Document, book: &Book, page: PageId, reports: Option<&ReportModels>) -> String {
    let n = book.number(doc, page);
    match sheets_of(doc, page, reports) {
        0 | 1 => n,
        k => format!("{n}\u{2013}{}", sheet_number(&n, k - 1)),
    }
}

/// The page whose continuation sheet (2nd or later) is numbered `number`, and that sheet's index:
/// continuation numbers count as taken (design 033 §5.3). O(generated pages × their sheets).
pub fn continuation_of(doc: &Document, book: &Book, number: &str, reports: Option<&ReportModels>) -> Option<(PageId, usize)> {
    book.order.iter().filter(|p| kind_of(doc, **p).is_some()).find_map(|p| {
        let n = book.number(doc, *p);
        (1..sheets_of(doc, *p, reports)).find(|i| sheet_number(&n, *i).eq_ignore_ascii_case(number.trim())).map(|i| (*p, i))
    })
}

/// The refusal of typing `number` when it is sheet `i` of `page` (design 033 §5.3).
pub fn continuation_taken(doc: &Document, number: &str, page: PageId, i: usize) -> String {
    let title = doc.pages.get(&page).map(|p| p.title.as_str()).unwrap_or_default();
    let owner = ecad_model::document::book::page_number(doc, page);
    format!("Number {number} is sheet {} of page {owner} ({title}).", i.saturating_add(1))
}

/// The Checks rows of continuation sheets that took another page's number (design 033 §5.3): a
/// table grew, so the book still prints and the two sheets share the number until the user
/// renumbers. Each row is on the other page. O(pages + generated pages × their sheets).
pub fn continuation_checks(doc: &Document, reports: Option<&ReportModels>) -> Vec<crate::checks::Check> {
    use crate::checks::{Check, Fix, Kind, Object};
    let book = Book::of(doc);
    let mut by_number: std::collections::HashMap<String, PageId> = std::collections::HashMap::new();
    for p in &book.order {
        by_number.entry(book.number(doc, *p).to_ascii_lowercase()).or_insert(*p);
    }
    let mut out = Vec::new();
    for p in book.order.iter().filter(|p| kind_of(doc, **p).is_some()) {
        let n = sheets_of(doc, *p, reports);
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
pub fn book_sheets_of(doc: &Document, reports: Option<&ReportModels>) -> usize {
    let extra: usize = doc.pages.iter().filter(|(_, p)| !p.kind.is_schematic()).map(|(id, _)| sheets_of(doc, *id, reports).saturating_sub(1)).sum();
    doc.page_order.len() + extra
}

/// Whether `ops` can change what a generated page of `kind` shows: [`touched_by`] on each of
/// their effects.
pub fn touches(ops: &[Op], kind: GenKind) -> bool {
    ecad_model::document::effect::Effects::of(ops).iter().any(|e| touched_by(e, kind))
}

/// Whether `effect` can change what a generated page of `kind` shows (beyond the frame's own map
/// and the report models, which the project watches itself). The cover's own map is
/// `crate::pages::front::touches` (brief 054b), which needs the document; the report kinds follow their
/// models.
pub fn touched_by(effect: &ecad_model::document::effect::Effect<'_>, kind: GenKind) -> bool {
    use ecad_model::document::effect::Effect;
    match (kind, effect) {
        (GenKind::Contents, Effect::Page { before: Some(b), after: Some(a), index_before: None, index_after: None, .. }) => {
            b.title != a.title || b.number != a.number || b.folder != a.folder
        }
        (GenKind::Contents, Effect::Page { .. } | Effect::Folder { .. }) => true,
        (GenKind::Revisions, Effect::Prop { object: ObjectRef::Project, .. } | Effect::Version { .. }) => true,
        // Design 072 §5.3: the revision rows and the project's info are the project's own data.
        (GenKind::Revisions, Effect::Project { before, after }) => before.revisions != after.revisions || before.info != after.info,
        _ => false,
    }
}

/// Whether the content of `connection_points` page `page` changed in the last update of `o` (design 046
/// §9): the table style when a strip it shows changed (028); the graphic style when a strip it
/// shows was drawn again, or the layout settings changed. Either style redraws when the project's
/// style changed and the page has no style of its own. O(strips shown).
pub fn connection_points_rewritten(doc: &Document, page: PageId, o: &ReportModels) -> bool {
    let sp = &o.strip_pages;
    let style = &crate::defaults::strip_pages::STYLE;
    let own_style = doc.pages.get(&page).and_then(|p| p.display.get(style.key)).is_some_and(|v| style.parse(v).is_some());
    if sp.style_changed && !own_style {
        return true;
    }
    let (changed, layout) = match crate::terminal_strips::pages::style_of(doc, page) {
        crate::terminal_strips::pages::Style::Table => (&o.strips.last_changed, false),
        crate::terminal_strips::pages::Style::Graphic => (&sp.last_changed, sp.layout_changed),
    };
    layout
        || match strip_of(doc, page) {
            Some(c) => changed.contains(&c),
            None => !changed.is_empty(),
        }
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

/// The drawing area of a sheet in paper millimetres: left, bottom, right, top.
pub fn area(input: &crate::pages::frames::FrameInput) -> (f64, f64, f64, f64) {
    let (w, h) = (f64::from(input.sheet.width), f64::from(input.sheet.height));
    // Brief 055 (FP4): the frame's margins and its block's height.
    let m = input.draw.margins;
    (m.left + 5.0, m.bottom + input.keep_out() + 5.0, w - m.right - 5.0, h - m.top - 5.0)
}

/// A table row: cells, and whether it is a heading (BOM location blocks, strips).
#[derive(Debug, Clone, Default)]
pub struct TRow {
    pub cells: Vec<String>,
    pub heading: bool,
    /// Brief 028: the terminal plan's marks.
    pub marks: Marks,
}

/// What a terminal plan row draws beyond its text (design 028 §5.3).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Marks {
    /// Column of the terminal box (its label centred in a 6 × 4 mm box).
    pub terminal_block_box: Option<usize>,
    /// The bridge group whose dot this row carries, in the bridge column.
    pub bridge: Option<(usize, usize)>,
    /// The cable column and whether this row starts or continues a cable's bracket.
    pub cable: Option<(usize, bool)>,
}

/// What a generated page shows: (columns with A3 widths, rows).
fn table(doc: &Document, kind: GenKind, reports: Option<&ReportModels>) -> (Vec<(String, f64)>, Vec<TRow>) {
    if kind == GenKind::Revisions {
        return revision_list(doc);
    }
    let (cols, rows) = table_static(doc, kind, reports);
    (cols.into_iter().map(|(n, w)| (n.to_string(), w)).collect(), rows)
}

/// The revision list (design 033 §7.3): the project's revision columns, then Version, across
/// the A3 area (Version keeps 60 mm).
fn revision_list(doc: &Document) -> (Vec<(String, f64)>, Vec<TRow>) {
    const VERSION_MM: f64 = 60.0;
    let row = |cells: Vec<String>| TRow { cells, ..TRow::default() };
    let columns = crate::releases::revisions::columns(doc);
    let (widths, _) = crate::releases::revisions::widths(&columns, A3_AREA - VERSION_MM);
    let mut cols: Vec<(String, f64)> = columns.iter().zip(&widths).map(|(c, w)| (c.header.clone(), *w)).collect();
    cols.push(("Version".into(), VERSION_MM));
    let issued = crate::releases::revisions::issued(doc);
    let rows = if issued.is_empty() {
        let mut cells = vec![String::new(); columns.len()];
        if let Some(i) = columns.iter().position(|c| c.field == "description")
            && let Some(c) = cells.get_mut(i)
        {
            *c = "No revisions issued yet.".into();
        }
        vec![row(cells)]
    } else {
        issued
            .iter()
            .map(|r| {
                let mut c: Vec<String> = columns.iter().map(|col| r.get(&col.field).to_string()).collect();
                c.push(r.issued.clone().unwrap_or_default());
                row(c)
            })
            .collect()
    };
    (cols, rows)
}

fn table_static(doc: &Document, kind: GenKind, reports: Option<&ReportModels>) -> (Vec<(&'static str, f64)>, Vec<TRow>) {
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
                    let title = doc.pages.get(p).map(|x| x.title.to_string()).unwrap_or_default();
                    row(vec![number_range(doc, &b, *p, reports), title, crate::releases::revisions::folder_path(doc, folder)])
                })
                .collect();
            (vec![("Page", 25.0), ("Title", 220.0), ("Folder", 145.0)], rows)
        }
        GenKind::Revisions => {
            let issued = crate::releases::revisions::issued(doc);
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
            // Brief 047 (design 047 §6.3): an Article column when `bom.column.article` is on (the
            // others narrower, still 390 mm), Qty with its unit, nested lines indented.
            let article = reports.is_some_and(|o| o.view.article);
            // UI rev 3 (board 1.15): the tag first, then the description, its quantity (right
            // aligned, [`BOM_QTY_COLUMN`]), maker and part number.
            let cols = if article {
                vec![("Tags", 60.0), ("Description", 120.0), ("Qty", 15.0), ("Maker", 60.0), ("Part number", 85.0), ("Article", 50.0)]
            } else {
                vec![("Tags", 70.0), ("Description", 140.0), ("Qty", 15.0), ("Maker", 70.0), ("Part number", 95.0)]
            };
            let Some(o) = reports else { return (cols, Vec::new()) };
            let mut rows = Vec::new();
            let mut last: Option<Option<String>> = None;
            let q = |v: &Option<String>| v.clone().unwrap_or_else(|| "?".into());
            for r in o.bom(Group::Location) {
                if last.as_ref() != Some(&r.location) {
                    rows.push(TRow { cells: vec![q(&r.location)], heading: true, ..TRow::default() });
                    last = Some(r.location.clone());
                }
                let mut cells = vec![r.tags.join(", "), q(&r.description_text()), r.qty_text(), q(&r.maker), q(&r.part_number)];
                if article {
                    cells.push(r.article_number.clone().unwrap_or_default());
                }
                rows.push(row(cells));
            }
            if rows.is_empty() {
                rows.push(row(vec![String::new(), "No devices yet.".into()]));
            }
            (cols, rows)
        }
        // Brief 028 (design 028 §5.3), drawn by `terminal_block_rows`.
        GenKind::TerminalBlocks => (TERMINAL_BLOCK_COLUMNS.to_vec(), Vec::new()),
        // Brief 051: drawn by `crate::plc::sheets`.
        GenKind::PlcRack => (Vec::new(), Vec::new()),
        // Brief 045c (design 045 §8.2).
        GenKind::IoList => {
            let Some(o) = reports else { return (crate::plc::iolist::PAGE_COLUMNS.to_vec(), Vec::new()) };
            let mut rows: Vec<TRow> =
                crate::plc::iolist::page_lines(doc, &o.plc).into_iter().map(|(heading, cells)| TRow { cells, heading, ..TRow::default() }).collect();
            if rows.is_empty() {
                rows.push(row(vec![String::new(), crate::plc::iolist::NO_CARDS.into()]));
            }
            (crate::plc::iolist::PAGE_COLUMNS.to_vec(), rows)
        }
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
            let Some(o) = reports else { return (cols, Vec::new()) };
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

/// The BOM page's Qty column, drawn right aligned (UI rev 3, board 1.15).
pub const BOM_QTY_COLUMN: usize = 2;

/// The terminal plan's columns with their A3 widths (design 028 §5.3).
const TERMINAL_BLOCK_COLUMNS: [(&str, f64); 9] = [
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
fn terminal_block_rows(doc: &Document, page: PageId, o: &ReportModels) -> Vec<TRow> {
    let style = doc.settings.reference_style();
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
                crate::terminal_strips::strip_model::PlanItem::Term(i, r) => (i, r),
                // Brief 030 (design 030 §7.3): a gap row is a full row, the word in Inside target,
                // a mark across Terminal, the part in Place.
                crate::terminal_strips::strip_model::PlanItem::Gap { gap, acc } => {
                    let mut cells = vec![String::new(); TERMINAL_BLOCK_COLUMNS.len()];
                    let words = if acc == crate::terminal_strips::accessories::Acc::StripLabel {
                        format!("strip label {}", s.tag)
                    } else {
                        acc.words().to_string()
                    };
                    let mark = match acc {
                        crate::terminal_strips::accessories::Acc::EndStop => "||",
                        crate::terminal_strips::accessories::Acc::EndPlate => "|",
                        crate::terminal_strips::accessories::Acc::Partition => "==",
                        _ => "[ ]",
                    };
                    let part = s.acc.gap_part(acc, crate::reports::gap_pe(s, gap)).cloned().unwrap_or_else(|| "?".into());
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
                let at = s.jumper_bars_at(ri);
                let lanes = at.iter().map(|(l, _)| l.lane + 1).max().unwrap_or(0);
                let mut m = vec![' '; lanes];
                for (l, on) in &at {
                    if let Some(x) = m.get_mut(l.lane) {
                        *x = if *on { 'o' } else { ':' };
                    }
                }
                m.into_iter().collect::<String>().trim_end().to_string()
            };
            // Brief 036: a PE mark only on a one-level PE terminal (an L/N/PE level shows `1.PE`).
            let pe_mark = r.pe && !r.ty.multi();
            if r.connection_point.is_none() {
                let mut cells = vec![String::new(); TERMINAL_BLOCK_COLUMNS.len()];
                if let Some(c) = cells.get_mut(0) {
                    *c = r.state().into();
                }
                if let Some(c) = cells.get_mut(2) {
                    *c = if pe_mark { format!("{} PE{marker}", r.shown) } else { format!("{}{marker}", r.shown) };
                }
                if let Some(c) = cells.get_mut(3) {
                    *c = links.clone();
                }
                rows.push(TRow { cells, heading: false, marks: Marks { terminal_block_box: Some(2), ..Marks::default() } });
                last_cable = None;
                continue;
            }
            let lines = r.inside.len().max(r.outside.len()).max(1);
            for i in 0..lines {
                let inside = r.inside.get(i);
                let outside = r.outside.get(i);
                let q = |v: Option<&String>| v.cloned().unwrap_or_else(|| "?".into());
                let label = if i == 0 { format!("{}{marker}", r.shown) } else { format!("{}\u{2032}", r.shown) };
                let label = if pe_mark { format!("{label} PE") } else { label };
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
                    terminal_block_box: (i == 0).then_some(2),
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

/// A table page laid out: its columns (A3 widths), rows, and sheets as [`paginate_with`] makes
/// them.
pub struct Laid {
    pub cols: Vec<(String, f64)>,
    pub rows: Vec<TRow>,
    pub sheets: Vec<(usize, usize, Option<String>)>,
}

impl Laid {
    pub fn of(doc: &Document, page: PageId, kind: GenKind, reports: Option<&ReportModels>) -> Laid {
        let (cols, mut rows) = table(doc, kind, reports);
        if kind == GenKind::TerminalBlocks
            && let Some(o) = reports
        {
            rows = terminal_block_rows(doc, page, o);
        }
        let heads: Vec<Option<String>> = rows.iter().map(|r| r.heading.then(|| r.cells.first().cloned().unwrap_or_default())).collect();
        let sheets = paginate_with(&heads, kind == GenKind::TerminalBlocks);
        Laid { cols, rows, sheets }
    }
}

/// The sheet windows of `page` in model coordinates: (bottom-left, top-right) per sheet.
pub fn sheet_windows_of(doc: &Document, page: PageId, reports: Option<&ReportModels>) -> Vec<(Pt, Pt)> {
    let spec = ecad_model::document::sheet::sheet_of(doc, page);
    let k = unit_scale(doc.settings.length_unit);
    let (w, h) = (f64::from(spec.width), f64::from(spec.height));
    (0..sheets_of(doc, page, reports))
        .map(|i| {
            let x0 = (w + SHEET_GAP) * i as f64;
            (Pt::new(x0 * k, 0.0), Pt::new((x0 + w) * k, h * k))
        })
        .collect()
}

/// How many generated values in `pages` are `?` (Print Book's warning line).
pub fn flagged(doc: &Document, pages: &BTreeSet<PageId>, reports: Option<&ReportModels>) -> usize {
    pages
        .iter()
        .map(|p| match kind_of(doc, *p) {
            Some(GenKind::BomLocation) => reports.map_or(0, |o| o.bom_table(Group::Location).flagged()),
            Some(GenKind::Cables) => reports.map_or(0, |o| o.cable_table().flagged()),
            _ => 0,
        })
        .sum()
}
