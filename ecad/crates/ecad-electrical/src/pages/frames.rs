//! What a page's frame and title block show: the
//! frame's input read from the document, its text styles, the update map of what can change it, and
//! fitting a value in its box. Drawing it is `frame`.

use std::collections::BTreeMap;

use ecad_model::document::book::Book;
use ecad_model::document::sheet::SheetSpec;
use ecad_model::{Document, LengthUnit, ObjectRef, Op, PageId};

use ecad_model::look::{FrameBlock, FrameDraw, FrameText, Look};

/// The reserved layer of the frame (`ecad.frame`), locked so CADCraft's own commands leave it.
pub const LAYER: &str = "ECAD-FRAME";

/// `ecad.frame`: ACI 7.
pub const FRAME_ACI: u8 = 7;

/// `ecad.frame-label` and `ecad.sheet-edge`: ACI 8.
pub const LABEL_ACI: u8 = 8;

/// `ecad.wire-weight`.
pub const THIN_WEIGHT: u16 = 25;

/// Refusal of an edit of the frame.
pub const DERIVED: &str = "The sheet frame is derived; change it in Project settings or the page's Sheet.";

/// Most issued revisions the title block lists.
pub const TABLE_ROWS: usize = 7;

/// Project property keys the title block shows.
pub const TITLE: &str = "title";

/// The text project property `key` holds: a field of the project's info:
/// `title`, `drawing_number`, `customer`, `site`, `status`, `cover.notes`), else the free
/// property. The one reader of both.
pub fn project_text(doc: &Document, key: &str) -> Option<String> {
    match <ecad_model::InfoField as ecad_model::Word>::parse(key) {
        Some(f) => doc.project().info.get(f).map(str::to_string),
        None => doc.display().get(key).cloned(),
    }
}

/// `project` with info field `f` set to `value` (`None` or blank clears it).
///
/// # Errors
/// A value the field cannot hold, in words.
pub(crate) fn with_info(
    project: &ecad_model::Project, f: ecad_model::InfoField, value: Option<&str>,
) -> Result<ecad_model::Project, crate::CmdError> {
    let info = project.info.with(f, value).map_err(|e| crate::CmdError::refused(format!("{} is refused: {e}.", ecad_model::Word::word(f))))?;
    Ok(ecad_model::Project { info, ..project.clone() })
}

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
    /// order.
    pub revisions: Vec<Vec<String>>,
    /// The project's revision columns.
    pub rev_columns: Vec<crate::releases::revisions::Column>,
    /// Whether the drawing changed since the newest issue: set by the project,
    /// which keeps the flag; `of` leaves it off.
    pub pending: bool,
    /// The frame's text styles, from the look.
    pub text: FrameText,
    /// The page's frame style (line weights, colour, grid labels).
    pub draw: FrameDraw,
    /// The block the frame draws (its own, or the built-in block for this sheet).
    pub block: FrameBlock,
    /// The project properties the block's fields read, by key (empty when unset): the
    /// block's bound key set.
    pub project: BTreeMap<String, String>,
    /// The newest issued revision's fields the block's fields read (besides `rev`).
    pub newest: BTreeMap<String, String>,
}

impl FrameInput {
    /// What `page`'s frame shows in `doc` (`book`: its book order), in `look`, with its text
    /// styles `text` ([`Look::frame_text`], or its [`FrameText::sizes`] for a rule that only reads
    /// the layout).
    pub fn with(doc: &Document, page: PageId, book: &Book, look: &dyn Look, text: FrameText) -> FrameInput {
        let prop = |k: &str| project_text(doc, k).unwrap_or_default();
        let issued = crate::releases::revisions::issued(doc);
        let rev_columns = crate::releases::revisions::columns(doc);
        let next_page = book.next(page);
        // Checked there, so the frame and the references always use the same sheet:
        // the frame's own sheet first.
        let sheet = ecad_model::document::sheet::sheet_of(doc, page);
        let draw = look.frame_draw(doc, page);
        // The block may be borrowed from another frame (page, page type, frame).
        let id = ecad_model::document::sheet::title_block_style(doc, page);
        let block = look.title_block(doc, &id, &sheet, &draw.margins, &text);
        let (project, newest) = crate::title_blocks::bind::read(doc, &block.layout);
        // The rows the block's tables can show.
        let rows = block
            .layout
            .items
            .iter()
            .filter_map(|i| match i {
                ecad_model::objects::title_block::BlockItem::RevTable { rows, .. } => Some(usize::from(*rows)),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            .min(usize::from(ecad_model::objects::title_block::MAX_TABLE_ROWS));
        FrameInput {
            sheet,
            unit: doc.settings().length_unit,
            project_title: prop(TITLE),
            customer: prop(CUSTOMER),
            drawing_number: prop(DRAWING_NUMBER),
            status: prop(STATUS),
            page_title: doc.pages().get(&page).map(|p| p.title.to_string()).unwrap_or_default(),
            // Stored as entry keys of the project's list; the title block shows codes.
            function: crate::locations::designations::shown_text(
                doc,
                crate::locations::designations::Aspect::Function,
                crate::locations::designations::inherited_key(doc, page, crate::locations::designations::Aspect::Function).as_deref(),
            ),
            location: crate::locations::designations::shown_text(
                doc,
                crate::locations::designations::Aspect::Location,
                crate::locations::designations::inherited_key(doc, page, crate::locations::designations::Aspect::Location).as_deref(),
            ),
            page: book.number(doc, page),
            rev: crate::releases::revisions::rev_box(doc, false),
            next_page,
            next: next_page.map_or_else(|| "-".to_string(), |n| book.number(doc, n)),
            total: book.order.len().to_string(),
            revisions: issued.iter().take(rows).map(|r| rev_columns.iter().map(|c| r.get(&c.field).to_string()).collect()).collect(),
            rev_columns,
            pending: false,
            text,
            draw,
            block,
            project,
            newest,
        }
    }
}

impl FrameInput {
    /// This input with the project's pending flag: the Rev box marked as the
    /// project's `revisions.pending` says.
    pub fn with_pending(mut self, doc: &Document, pending: bool) -> FrameInput {
        self.pending = pending;
        self.rev = crate::releases::revisions::rev_box(doc, pending);
        self
    }

    /// The keep-out strip at the bottom of the frame, mm: the block's height
    /// (the built-in's 40 mm), never more than the frame's height. As
    /// `ecad_model::document::sheet::keep_out`.
    pub fn keep_out(&self) -> f64 {
        let h = self.block.layout.height().unwrap_or(ecad_model::document::sheet::TITLE_BLOCK_MM);
        let m = self.draw.margins;
        h.min((f64::from(self.sheet.height) - m.bottom - m.top).max(0.0))
    }
}

/// Whether `ops` can change the frame of `page`, whose frame last showed `shown`:
/// [`touched_by`] on each of their effects.
pub fn touches(ops: &[Op], page: PageId, shown: &FrameInput) -> bool {
    ecad_model::document::effect::Effects::of(ops).iter().any(|e| touched_by(e, page, shown))
}

/// Whether `effect` can change the frame of `page`, whose frame last showed `shown`
/// the frame and title block rows, `FrameView`). Every effect not listed
/// changes nothing there.
pub fn touched_by(effect: &ecad_model::document::effect::Effect<'_>, page: PageId, shown: &FrameInput) -> bool {
    use ecad_model::document::effect::Effect;
    match effect {
        Effect::DeviceLink { .. } => false,
        Effect::Settings { before, after } => before.sheet != after.sheet || before.length_unit != after.length_unit,
        // Only what the title block shows: its fields, the defaults pages inherit, and issuing.
        // A text size or the font (the `text.` keys).
        Effect::Prop { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if ecad_model::look::is_reserved_key(key.raw) => true,
        // The frame style choice, and any frame or text style record (the frame's
        // look and its texts' styles).
        Effect::Prop { object: ObjectRef::Project | ObjectRef::Standard, key, .. } if key.raw == ecad_model::document::sheet::FRAME_STYLE_KEY => true,
        // A page type's sheet, frame, title block or zones.
        Effect::Style { catalogue, .. } => {
            *catalogue == ecad_model::StyleCatalogue::Frame
                || *catalogue == ecad_model::StyleCatalogue::Text
                || *catalogue == ecad_model::StyleCatalogue::Page
        }
        // Switching one standard for another may change its frame records and
        // its `print.frame.style`.
        Effect::Standard { before, after } => before != after,
        // An issue (the Rev box and the table), the project's info, the project's
        // default, or a code the block shows renamed
        Effect::Project { before, after } if crate::releases::revisions::issue_changed(before, after) || before.info != after.info => true,
        Effect::Project { before, after } => crate::locations::designations::Aspect::ALL.into_iter().any(|a| {
            let field = match a {
                crate::locations::designations::Aspect::Function => &shown.function,
                crate::locations::designations::Aspect::Location => &shown.location,
            };
            crate::locations::designations::default_key(before, a) != crate::locations::designations::default_key(after, a)
                || crate::locations::designations::changed_codes(before, after, a).iter().any(|k| {
                    let old = crate::locations::designations::code_in_project(before, a, k);
                    old.as_deref().is_some_and(|b| b == field || shown.project.values().any(|v| v == b))
                })
        }),
        // A project key the block's fields read (its bound key
        // set), and the revision settings; any other key redraws
        // nothing.
        Effect::Prop { object: ObjectRef::Project, key, .. } => {
            shown.project.contains_key(key.raw)
                || key.raw == crate::releases::revisions::COLUMNS_KEY
                || key.raw == crate::releases::revisions::PENDING_KEY
        }
        Effect::Prop { object: ObjectRef::Page(p), .. } => *p == page,
        // A page changed in place; a folder move changes the book order (Next,
        // the numbers of unnumbered pages), a number the Next box of the page before it.
        Effect::Page { id, before: Some(b), after: Some(a), index_before: None, index_after: None } => {
            let ch = b.change(a);
            *id == page || ch.folder || (ch.number && shown.next_page == Some(*id))
        }
        // A page removed and added again: renamed in place changes only its own title box; moved
        // changes the book order (Next, and the numbers of unnumbered pages).
        Effect::Page { id, before: Some(_), after: Some(_), index_before, index_after } => *id == page || index_before != index_after,
        Effect::Page { .. } | Effect::Folder { .. } | Effect::Version { .. } => true,
        Effect::Prop { .. }
        | Effect::Layer { .. }
        | Effect::Item { .. }
        | Effect::Device { .. }
        | Effect::DeviceDefinition { .. }
        | Effect::ConnectionPoint { .. }
        | Effect::Rep { .. }
        | Effect::Wire { .. }
        | Effect::Route { .. }
        | Effect::Cable { .. }
        | Effect::Aid { .. }
        | Effect::LoosePart { .. }
        | Effect::Definition { .. } => false,
    }
}

/// Roughly how wide a character of the standard font is, per unit height.
const CHAR_WIDTH: f64 = 0.8;

/// The narrowest width factor a title-block text is squeezed to before it is cut.
const MIN_WIDTH_FACTOR: f64 = 0.6;

/// `value` fitted in `room` mm at `height`: narrowed down to
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
