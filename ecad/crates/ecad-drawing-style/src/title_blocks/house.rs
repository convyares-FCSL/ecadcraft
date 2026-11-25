//! The built-in house block.
//!
//! Our own arrangement of what a good drawing's title block holds: a revision grid with an ECR
//! column, a red status field, the function and location, page, next and total counters, the
//! page title and the drawing number, a logo box with an owner line. 40 mm high, as the standard block,
//! so nothing on a page moves. Columns from the right: counters 30 mm, drawing 90 mm (its Rev box 25 mm), project
//! 80 mm; the revision table takes the rest. Rows from the top: 15, 12.5 and 12.5 mm.
//!
//! The text sizes are the frame's text styles (`FrameText`), so Standards ▸ Text styles still
//! changes them on the built-in.

use ecad_model::document::project::{InfoField, RevField};
use ecad_model::document::sheet::TITLE_BLOCK_MM;
use ecad_model::objects::title_block::{
    Align, Binding, BlockItem, IfEmpty, Ink, LineWeight, MAX_TEXT_MM, MIN_TEXT_MM, Newest, PageField, PrintField, ProjectKey, Rect, RevisionColumn,
    TextDraw, TitleBlockLayout,
};

use ecad_electrical::pages::frames::TABLE_ROWS;
use ecad_model::look::FrameText;

/// Widths of the fixed columns, right to left (mm).
pub(crate) const COUNTERS_MM: f64 = 30.0;
pub(crate) const DRAWING_MM: f64 = 90.0;
pub(crate) const PROJECT_MM: f64 = 80.0;
/// The Rev box's width (mm): wide enough for `E (draft)` at the Rev size.
pub(crate) const REV_MM: f64 = 25.0;
/// Heights of the rows, top to bottom (mm).
pub(crate) const TOP_ROW_MM: f64 = 15.0;
pub(crate) const ROW_MM: f64 = 12.5;
/// Narrowest revision table the house block keeps (mm).
pub(crate) const MIN_TABLE_MM: f64 = 60.0;

fn draw(size: f64, bold: bool, align: Align) -> TextDraw {
    // The text styles keep their own bounds; a block keeps its own.
    let size = if size.is_finite() { size.clamp(MIN_TEXT_MM, MAX_TEXT_MM) } else { MIN_TEXT_MM };
    TextDraw { size: (size * 100.0).round() / 100.0, bold, align, colour: Ink::Ink }
}

fn field(at: Rect, bind: Binding, label: &str, draw: TextDraw) -> BlockItem {
    BlockItem::Field { at, bind, label: Some(label.to_string()), draw, if_empty: IfEmpty::Blank }
}

fn page(f: PageField) -> Binding {
    Binding::Page(f)
}

fn info(f: InfoField) -> Binding {
    Binding::Project(ProjectKey::of_info(f))
}

fn line(a: (f64, f64), b: (f64, f64)) -> BlockItem {
    BlockItem::Line { a, b, weight: LineWeight::Thin }
}

/// The house block for a frame `width` mm wide, with `text`'s sizes. Narrower frames than the
/// fixed columns need keep the revision table at [`MIN_TABLE_MM`] and let the block run past the
/// frame's left edge (a look, never a failure).
pub fn house(width: f64, text: &FrameText) -> TitleBlockLayout {
    let fixed = COUNTERS_MM + DRAWING_MM + PROJECT_MM;
    let w = if width.is_finite() { width.max(fixed + MIN_TABLE_MM) } else { fixed + MIN_TABLE_MM };
    let h = TITLE_BLOCK_MM;
    let r = w;
    let (xc, xd, xp) = (r - COUNTERS_MM, r - COUNTERS_MM - DRAWING_MM, r - fixed);
    let (y1, y2) = (h - TOP_ROW_MM, h - TOP_ROW_MM - ROW_MM);
    let value = text.value.mm;
    let main = text.main.mm;
    let mut items = vec![
        BlockItem::Line { a: (0.0, h), b: (r, h), weight: LineWeight::Thick },
        // Column dividers, full height.
        line((xc, 0.0), (xc, h)),
        line((xd, 0.0), (xd, h)),
        line((xp, 0.0), (xp, h)),
        // Row dividers of the three right columns.
        line((xp, y1), (r, y1)),
        line((xp, y2), (r, y2)),
        // Drawing column: Drawing no. | Rev, then Function | Location | Size.
        line((r - COUNTERS_MM - REV_MM, y2), (r - COUNTERS_MM - REV_MM, y1)),
        line((xd + 32.5, 0.0), (xd + 32.5, y2)),
        line((r - COUNTERS_MM - REV_MM, 0.0), (r - COUNTERS_MM - REV_MM, y2)),
        // Project column: Customer | Status.
        line((xp + 45.0, 0.0), (xp + 45.0, y2)),
        // The revision table: header and rows, newest at the top.
        BlockItem::RevTable { at: Rect::new(0.0, 0.0, xp, h), rows: TABLE_ROWS as u8, header: true, newest: Newest::Top },
        // Counters.
        field(Rect::new(xc, y1, COUNTERS_MM, TOP_ROW_MM), page(PageField::Number), "Page", draw(text.page.mm, text.page.bold(), Align::Centre)),
        field(Rect::new(xc, y2, COUNTERS_MM, ROW_MM), page(PageField::Next), "Next", draw(value, text.value.bold(), Align::Centre)),
        field(Rect::new(xc, 0.0, COUNTERS_MM, ROW_MM), page(PageField::Total), "Of", draw(value, text.value.bold(), Align::Centre)),
        // Drawing.
        field(Rect::new(xd, y1, DRAWING_MM, TOP_ROW_MM), page(PageField::Title), "Title", draw(main, text.main.bold(), Align::Left)),
        field(
            Rect::new(xd, y2, DRAWING_MM - REV_MM, ROW_MM),
            info(InfoField::DrawingNumber),
            "Drawing no.",
            draw(main, text.main.bold(), Align::Left),
        ),
        field(
            Rect::new(r - COUNTERS_MM - REV_MM, y2, REV_MM, ROW_MM),
            Binding::Revision(RevisionColumn::Field(RevField::Rev)),
            "Rev",
            draw(text.rev.mm, text.rev.bold(), Align::Centre),
        ),
        field(Rect::new(xd, 0.0, 32.5, ROW_MM), page(PageField::Function), "Function", draw(value, text.value.bold(), Align::Left)),
        field(Rect::new(xd + 32.5, 0.0, 32.5, ROW_MM), page(PageField::Location), "Location", draw(value, text.value.bold(), Align::Left)),
        field(
            Rect::new(r - COUNTERS_MM - REV_MM, 0.0, REV_MM, ROW_MM),
            Binding::Print(PrintField::Sheet),
            "Size",
            draw(value, text.value.bold(), Align::Centre),
        ),
        // Project: a logo box and the owner line, the project, the customer and the status.
        BlockItem::Logo { at: Rect::new(xp + 2.0, y1 + 4.0, PROJECT_MM - 4.0, TOP_ROW_MM - 5.0), name: String::new(), prims: Vec::new() },
        // The owner line: `company` is a free project key, not an info field.
        BlockItem::Field {
            at: Rect::new(xp + 2.0, y1 + 0.5, PROJECT_MM - 4.0, 3.0),
            bind: ProjectKey::new("company").map_or_else(|| info(InfoField::Customer), Binding::Project),
            label: None,
            draw: draw(text.caption.mm, text.caption.bold(), Align::Centre),
            if_empty: IfEmpty::Blank,
        },
        field(Rect::new(xp, y2, PROJECT_MM, ROW_MM), info(InfoField::Title), "Project", draw(main, text.main.bold(), Align::Left)),
        field(Rect::new(xp, 0.0, 45.0, ROW_MM), info(InfoField::Customer), "Customer", draw(value, text.value.bold(), Align::Left)),
    ];
    let mut status = draw(value, text.value.bold(), Align::Left);
    status.colour = Ink::Red;
    items.push(field(Rect::new(xp + 45.0, 0.0, 35.0, ROW_MM), info(InfoField::Status), "Status", status));
    TitleBlockLayout { size: (w, h), items }
}
