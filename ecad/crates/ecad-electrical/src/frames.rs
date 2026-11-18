//! What a page's frame and title block show (design 019, 033, 055; design 065 §3, cut 10): the
//! frame's input read from the document, its text looks, the update map of what can change it, and
//! fitting a value in its box. Drawing it is `frame`.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::book::Book;
use ecad_model::sheet::{SheetSpec, unit_scale};
use ecad_model::{Document, LengthUnit, ObjectRef, Op, PageId};

use crate::text_style::fonts::{Look, TextMetrics};
use crate::text_style::{TextKind, Weight};

/// The reserved layer of the frame (`ecad.frame`), locked so CADCraft's own commands leave it.
pub const LAYER: &str = "ECAD-FRAME";

/// `ecad.frame`: ACI 7.
pub const FRAME_ACI: u8 = 7;

/// `ecad.frame-label` and `ecad.sheet-edge`: ACI 8.
pub const LABEL_ACI: u8 = 8;

/// `ecad.frame-weight` (hundredths of a millimetre).
pub const FRAME_WEIGHT: u16 = 50;

/// `ecad.wire-weight`.
pub const THIN_WEIGHT: u16 = 25;

/// Refusal of an edit of the frame (design 019 §3.2).
pub const DERIVED: &str = "The sheet frame is derived; change it in Project settings or the page's Sheet.";

/// Most issued revisions the title block lists (owner answer Q12).
pub const TABLE_ROWS: usize = 7;

/// Project property keys the title block shows.
pub const TITLE: &str = "title";

pub const CUSTOMER: &str = "customer";

pub const DRAWING_NUMBER: &str = "drawing_number";

pub const STATUS: &str = "status";

/// Everything one page's frame shows.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameInput {
    pub sheet: SheetSpec,
    pub unit: LengthUnit,
    pub project_title: String,
    pub customer: String,
    pub drawing_number: String,
    pub status: String,
    pub page_title: String,
    pub function: String,
    pub location: String,
    pub page: String,
    pub rev: String,
    /// The next page in book order, and its number.
    pub next_page: Option<PageId>,
    pub next: String,
    pub total: String,
    /// Newest issued first, at most [`TABLE_ROWS`]: each row's values in [`Self::rev_columns`]
    /// order (brief 033).
    pub revisions: Vec<Vec<String>>,
    /// The project's revision columns (design 033 §7).
    pub rev_columns: Vec<crate::revisions::Column>,
    /// Whether the drawing changed since the newest issue (design 033 §6): set by the project,
    /// which keeps the flag; `of` leaves it off.
    pub pending: bool,
    /// The frame's text looks, from lane T's getter (design 033 §1.2).
    pub text: FrameText,
    /// Brief 050a: the page's frame style (line weights, colour, grid labels).
    pub look: crate::styles::frame::FrameLook,
    /// Brief 055: the block the frame draws (its own, or the house block for this sheet).
    pub block: crate::title_block::Block,
    /// Brief 055: the project properties the block's fields read, by key (empty when unset): the
    /// block's bound key set (design 055 §10.1).
    pub project: BTreeMap<String, String>,
    /// Brief 055: the newest issued revision's fields the block's fields read (besides `rev`).
    pub newest: BTreeMap<String, String>,
}

/// A text kind's look where ECAD draws in paper millimetres (frame, generated pages): its height
/// in millimetres, its style name, and the lineweight of bold drawn in the stroke font.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLook {
    pub mm: f64,
    pub style: &'static str,
    pub weight: Option<u16>,
}

/// The frame's text looks (design 033 §1.2): grid labels, box captions, values and revision rows,
/// Project and Page title, the Page number box, the Rev box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameText {
    pub grid: TextLook,
    pub caption: TextLook,
    pub value: TextLook,
    pub main: TextLook,
    pub page: TextLook,
    pub rev: TextLook,
    /// Brief 055: the lineweight bold is drawn with in the title fonts, for a block's bold texts
    /// when the main style is not bold itself.
    pub bold_weight: Option<u16>,
}

impl TextLook {
    /// Whether its style is bold (the CADCraft style name of a bold weight; brief 066d: a block
    /// item's `bold` comes from its kind's style, never a literal).
    pub fn bold(&self) -> bool {
        self.style.ends_with("BOLD")
    }

    /// `kind`'s look in a project of length unit `unit`, from its resolved `look`.
    pub fn of(look: &Look, kind: TextKind, unit: LengthUnit) -> TextLook {
        let s = look.style(kind);
        let mm = height_mm(s.height, unit);
        // Lane T's per-kind fonts decide whether bold is drawn as a heavier stroke (design 031
        // §2.2), as the engine's `LookApply::apply` does.
        let weight = match s.weight {
            Weight::Bold => look.fonts_for(kind).bold_lineweight,
            Weight::Regular => None,
        };
        TextLook { mm, style: s.style_name, weight }
    }
}

/// A text height `height` in drawing units of a project of length unit `unit`, in paper millimetres.
pub fn height_mm(height: f64, unit: LengthUnit) -> f64 {
    let k = unit_scale(unit);
    if k.is_finite() && k > 0.0 { height / k } else { height }
}

impl FrameText {
    /// The looks of `doc`'s frame, with the fonts `m` finds.
    pub fn with(doc: &Document, m: &dyn TextMetrics) -> FrameText {
        let look = Look::with(doc, m);
        let u = doc.settings.length_unit;
        let t = |k| TextLook::of(&look, k, u);
        FrameText {
            grid: t(TextKind::Grid),
            caption: t(TextKind::TitleCaption),
            value: t(TextKind::TitleValue),
            main: t(TextKind::TitleMain),
            page: t(TextKind::TitlePage),
            rev: t(TextKind::TitleRev),
            bold_weight: look.fonts_for(TextKind::TitleMain).bold_lineweight,
        }
    }

    /// The looks of `doc`'s frame without the fonts: sizes and style names, and no bold
    /// lineweights (those exist only when bold is drawn in the stroke font). What a rule that
    /// reads the block's layout and values needs (the title block checks); drawing uses
    /// [`FrameText::with`].
    pub fn sizes(doc: &Document) -> FrameText {
        let t = FrameText::with(doc, &AllFound);
        let plain = |l: TextLook| TextLook { weight: None, ..l };
        FrameText {
            grid: plain(t.grid),
            caption: plain(t.caption),
            value: plain(t.value),
            main: plain(t.main),
            page: plain(t.page),
            rev: plain(t.rev),
            bold_weight: None,
        }
    }
}

/// Fonts that are all found, so nothing is drawn in the stroke font: [`FrameText::sizes`].
struct AllFound;

impl TextMetrics for AllFound {
    fn found(&self, _name: &str) -> bool {
        true
    }
    fn width(&self, _name: &str, _text: &str, _height: f64) -> f64 {
        0.0
    }
    fn stroke_font(&self) -> &str {
        ""
    }
}

impl FrameInput {
    /// What `page`'s frame shows in `doc` (`book`: its book order), with its text looks `text`
    /// ([`FrameText::with`], or [`FrameText::sizes`] for a rule that only reads the layout).
    pub fn with(doc: &Document, page: PageId, book: &Book, text: FrameText) -> FrameInput {
        let prop = |k: &str| doc.properties.get(k).cloned().unwrap_or_default();
        let issued = crate::revisions::issued(doc);
        let rev_columns = crate::revisions::columns(doc);
        let next_page = book.next(page);
        // Checked there, so the frame and the references always use the same sheet (brief 055:
        // the frame's own sheet first).
        let sheet = ecad_model::sheet::sheet_of(doc, page);
        let look = crate::styles::frame::frame_look(doc, page);
        // The block may be borrowed from another frame (overseer ruling: page, page type, frame).
        let id = ecad_model::sheet::title_block_style(doc, page);
        let block = crate::title_block::block_of(doc, &id, &sheet, &look.margins, &text);
        let (project, newest) = crate::title_block::bind::read(doc, &block.layout);
        // The rows the block's tables can show.
        let rows = block
            .layout
            .items
            .iter()
            .filter_map(|i| match i {
                ecad_model::title_block::BlockItem::RevTable { rows, .. } => Some(usize::from(*rows)),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            .min(usize::from(ecad_model::title_block::MAX_TABLE_ROWS));
        FrameInput {
            sheet,
            unit: doc.settings.length_unit,
            project_title: prop(TITLE),
            customer: prop(CUSTOMER),
            drawing_number: prop(DRAWING_NUMBER),
            status: prop(STATUS),
            page_title: doc.pages.get(&page).map(|p| p.title.to_string()).unwrap_or_default(),
            // Brief 052: stored as entry keys of the project's list; the title block shows codes.
            function: crate::designations::shown_text(
                doc,
                crate::designations::Aspect::Function,
                crate::designations::inherited_key(doc, page, crate::designations::Aspect::Function).as_deref(),
            ),
            location: crate::designations::shown_text(
                doc,
                crate::designations::Aspect::Location,
                crate::designations::inherited_key(doc, page, crate::designations::Aspect::Location).as_deref(),
            ),
            page: book.number(doc, page),
            rev: crate::revisions::rev_box(doc, false),
            next_page,
            next: next_page.map_or_else(|| "-".to_string(), |n| book.number(doc, n)),
            total: book.order.len().to_string(),
            revisions: issued.iter().take(rows).map(|r| rev_columns.iter().map(|c| r.get(&c.field).to_string()).collect()).collect(),
            rev_columns,
            pending: false,
            text,
            look,
            block,
            project,
            newest,
        }
    }
}

impl FrameInput {
    /// This input with the project's pending flag (design 033 §6): the Rev box marked as the
    /// project's `revisions.pending` says.
    pub fn with_pending(mut self, doc: &Document, pending: bool) -> FrameInput {
        self.pending = pending;
        self.rev = crate::revisions::rev_box(doc, pending);
        self
    }

    /// Brief 055 (FP4): the keep-out strip at the bottom of the frame, mm: the block's height
    /// (the built-in's 40 mm), never more than the frame's height. As
    /// `ecad_model::sheet::keep_out`.
    pub fn keep_out(&self) -> f64 {
        let h = self.block.layout.height().unwrap_or(ecad_model::sheet::TITLE_BLOCK_MM);
        let m = self.look.margins;
        h.min((f64::from(self.sheet.height) - m.bottom - m.top).max(0.0))
    }
}

/// Whether `ops` can change the frame of `page`, whose frame
/// last showed `shown` (design 019 §9, the frame and title block rows). Every op not listed
/// changes nothing there.
pub fn touches(ops: &[Op], page: PageId, shown: &FrameInput) -> bool {
    // A rename or move is a RemovePage + AddPage pair of one page.
    let removed: BTreeSet<PageId> = ops.iter().filter_map(|o| if let Op::RemovePage { id, .. } = o { Some(*id) } else { None }).collect();
    let added: BTreeSet<PageId> = ops.iter().filter_map(|o| if let Op::AddPage { id, .. } = o { Some(*id) } else { None }).collect();
    let pair = |id: &PageId| removed.contains(id) && added.contains(id);
    ops.iter().any(|op| match op {
        Op::SetProjectSettings { before, after } => before.sheet != after.sheet || before.length_unit != after.length_unit,
        // Only what the title block shows: its fields, the defaults pages inherit, and issuing.
        // Brief 033 (design 033 §9.1): a text size or the font (lane T's `text.` keys).
        Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if crate::text_style::is_reserved_key(key) => true,
        // Brief 050a: the frame style choice, and any frame or text style record (the frame's
        // look and its texts' styles).
        Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if key == crate::defaults::styles::FRAME.key => true,
        // Brief 054: a page type's sheet, frame, title block or zones.
        Op::SetStyle { catalogue, .. } => {
            *catalogue == ecad_model::StyleCatalogue::Frame
                || *catalogue == ecad_model::StyleCatalogue::Text
                || *catalogue == ecad_model::StyleCatalogue::Page
        }
        // Review of #110, 2: switching one standard for another may change its frame records and
        // its `print.frame.style`.
        Op::SetStandard { before, after } => before != after,
        // Brief 052 (design 052 §8.2a, tester 052 finding 1): a rename in the project's list
        // reaches the title block when it shows the entry's old code.
        // Design 072 §5.3: the project's default, or a code it shows renamed.
        Op::ModifyProject { before, after } => crate::designations::Aspect::ALL.into_iter().any(|a| {
            let field = match a {
                crate::designations::Aspect::Function => &shown.function,
                crate::designations::Aspect::Location => &shown.location,
            };
            crate::designations::default_key(before, a) != crate::designations::default_key(after, a)
                || crate::designations::changed_codes(before, after, a).iter().any(|k| {
                    let old = crate::designations::code_in_project(before, a, k);
                    old.as_deref().is_some_and(|b| b == field || shown.project.values().any(|v| v == b))
                })
        }),
        // Brief 055 (design 055 §10.1): a project key the block's fields read (its bound key
        // set), the defaults pages inherit, and the revision keys; any other key redraws nothing.
        Op::SetProperty { object: ObjectRef::Project, key, .. } => {
            shown.project.contains_key(key.as_str())
                // Brief 033 (design 033 §9.1).
                || key == crate::revisions::COLUMNS_KEY
                || key == crate::revisions::PENDING_KEY
                || (key.starts_with(crate::revisions::PREFIX) && key.ends_with(".issued"))
        }
        Op::SetProperty { object: ObjectRef::Page(p), .. } => *p == page,
        // Design 072 §5.1: a page changed in place; a folder move changes the book order (Next,
        // the numbers of unnumbered pages), a number the Next box of the page before it.
        Op::ModifyPage { id, before, after } => {
            let ch = before.change(after);
            *id == page || ch.folder || (ch.number && shown.next_page == Some(*id))
        }
        // A page renamed in place changes only its own title box; a page moved changes the book
        // order (Next, and the numbers of unnumbered pages).
        Op::AddPage { id, index, .. } | Op::RemovePage { id, index, .. } if pair(id) => {
            let moved = ops.iter().any(|o| match o {
                Op::AddPage { id: a, index: i, .. } | Op::RemovePage { id: a, index: i, .. } => a == id && i != index,
                _ => false,
            });
            *id == page || moved
        }
        Op::AddPage { .. } | Op::RemovePage { .. } => true,
        Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => true,
        Op::NameVersion { .. } | Op::UnnameVersion { .. } => true,
        _ => false,
    })
}

/// Roughly how wide a character of the standard font is, per unit height.
const CHAR_WIDTH: f64 = 0.8;

/// The narrowest width factor a title-block text is squeezed to before it is cut.
const MIN_WIDTH_FACTOR: f64 = 0.6;

/// `value` fitted in `room` mm at `height` (design 019 §4): narrowed down to
/// `MIN_WIDTH_FACTOR`, then cut with `…`.
pub fn fit(value: &str, height: f64, room: Option<f64>) -> (String, f64) {
    let Some(room) = room.filter(|r| *r > 0.0) else { return (value.to_string(), 1.0) };
    let n = value.chars().count() as f64;
    let wide = n * height * CHAR_WIDTH;
    if wide <= room {
        return (value.to_string(), 1.0);
    }
    let wf = room / wide;
    if wf >= MIN_WIDTH_FACTOR {
        return (value.to_string(), wf);
    }
    let keep = ((room / (height * CHAR_WIDTH * MIN_WIDTH_FACTOR)).floor() as usize).saturating_sub(1).max(1);
    let mut s: String = value.chars().take(keep).collect();
    s.push('…');
    (s, MIN_WIDTH_FACTOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_narrowed_then_cut() {
        assert_eq!(fit("abc", 2.5, Some(100.0)), ("abc".to_string(), 1.0));
        let (s, wf) = fit("abcdefghij", 2.5, Some(15.0));
        assert_eq!(s, "abcdefghij");
        assert!((wf - 0.75).abs() < 1e-9);
        let (s, wf) = fit(&"x".repeat(100), 2.5, Some(20.0));
        assert!(s.ends_with('…') && s.chars().count() < 20);
        assert!((wf - MIN_WIDTH_FACTOR).abs() < 1e-9);
    }
}
