//! The title block rows of the Checks list.
//!
//! - `title-value-cut` (Info): a bound value is cut in the title block on at least one page; one
//!   row per frame and field, counting the pages. Go to: the first page; Fix: **Edit block**.
//! - `title-binding-unknown` (Info): a frame has fields this build cannot fill (an unknown source
//!   or key, a newer file), or items past the bounds of a title block, or a title block on a
//!   record that is not a frame. One row per record.
//! - `frame-sheet-differs` (Warning): a page's own `sheet` differs from its frame's; the frame
//!   wins. Fix: **Clear the page's sheet**.
//!
//! # Update map
//! Worked out on open, reload or first show, and again only after an effect [`touches`] names: a
//! frame or text style record, the standard, a frame choice, a page added, removed or changed
//! (title, number, sheet, function, location), a project property, the project's sheet or unit.
//! Every other effect leaves the rows. Working them out reads each page's values once and fits each
//! field's value in its box; no frame is drawn (one frame input per frame style and sheet).

use std::collections::BTreeMap;

use ecad_model::document::book::Book;
use ecad_model::document::effect::Effect;
use ecad_model::document::sheet;
use ecad_model::objects::styles::StyleLayer;
use ecad_model::objects::title_block::BlockItem;
use ecad_model::{Document, ObjectRef, PageId};

use crate::checks::{Check, Fix, Kind, Object};
use crate::pages::frames::FrameInput;

/// Most pages the cut check reads (a guard on a hostile file; the check stays bounded).
pub const MAX_PAGES: usize = 10_000;

/// Whether `effect` can change the title block rows.
pub fn touches(effect: &Effect<'_>) -> bool {
    match effect {
        Effect::Style { .. } | Effect::Standard { .. } => true,
        Effect::Settings { before, after } => before.sheet != after.sheet || before.length_unit != after.length_unit,
        Effect::Prop { object: ObjectRef::Project | ObjectRef::Standard | ObjectRef::Page(_), .. } => true,
        // The info, the revision rows and the lists the block shows.
        Effect::Project { .. } => true,
        Effect::Page { .. } | Effect::Folder { .. } => true,
        Effect::Layer { .. }
        | Effect::Item { .. }
        | Effect::Device { .. }
        | Effect::DeviceDefinition { .. }
        | Effect::ConnectionPoint { .. }
        | Effect::Rep { .. }
        | Effect::Wire { .. }
        | Effect::Route { .. }
        | Effect::Cable { .. }
        | Effect::Prop { .. }
        | Effect::Definition { .. }
        | Effect::Version { .. }
        | Effect::Aid { .. }
        | Effect::LoosePart { .. }
        | Effect::DeviceLink { .. } => false,
    }
}

/// The title block rows as a [`crate::checks::CheckSource`]: kept on pages with
/// checks off.
pub(crate) struct TitleBlockRows;

impl crate::checks::CheckSource for TitleBlockRows {
    fn key(&self) -> crate::checks::Key {
        crate::checks::Key::TitleBlocks
    }
    fn touched(&self, scope: &crate::checks::Scope) -> bool {
        scope.title_blocks
    }
    fn kept_on_pages_off(&self) -> bool {
        true
    }
    fn checks(&self, doc: &Document) -> Vec<Check> {
        rows(doc)
    }
}

/// Every title block row of `doc`.
pub fn rows(doc: &Document) -> Vec<Check> {
    let mut out = unknown_rows(doc);
    out.extend(sheet_rows(doc));
    out.extend(cut_rows(doc));
    out
}

/// `title-binding-unknown`: one row per record whose block this build cannot fully draw.
fn unknown_rows(doc: &Document) -> Vec<Check> {
    let mut out = Vec::new();
    for layer in [StyleLayer::Project, StyleLayer::Standard] {
        let Some(cats) = ecad_model::objects::styles::layer_of(doc, layer) else { continue };
        for (cat, records) in cats {
            for (id, r) in records.iter().take(ecad_model::objects::styles::MAX_STYLES_PER_CATALOGUE) {
                // Only a frame style's body holds a title block.
                let Some(l) = r.title_block() else { continue };
                let name = ecad_model::registry::styles::name(doc, crate::styles::catalogue(*cat), id);
                // A size past the bounds is drawn with the built-in's
                // keep-out strip; say so.
                if !l.size_ok() {
                    let which = format!(
                        "{name}'s title block size is out of bounds (1 to {} mm wide, 1 to {} mm high); its pages keep the built-in's 40 mm strip.",
                        ecad_model::objects::title_block::MAX_COORD_MM,
                        ecad_model::objects::title_block::MAX_BLOCK_HEIGHT_MM
                    );
                    out.push(Check { kind: Kind::TitleBindingUnknown, page: None, at: None, object: Object::None, which, fix: None });
                }
                // Every binding that reads can be filled; only items past the
                // bounds are skipped.
                let (_, skipped) = l.usable();
                let n = skipped.items;
                if n == 0 {
                    continue;
                }
                let which = format!("{name} has {} this build cannot draw.", count(n, "item", "items"));
                out.push(Check { kind: Kind::TitleBindingUnknown, page: None, at: None, object: Object::None, which, fix: None });
            }
        }
    }
    out
}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

/// `frame-sheet-differs`: pages whose own sheet differs from their frame's.
fn sheet_rows(doc: &Document) -> Vec<Check> {
    let mut out = Vec::new();
    for (page, p) in doc.pages().iter().take(MAX_PAGES) {
        let Some(own) = p.sheet else { continue };
        let Some(framed) = sheet::frame_sheet(doc, *page) else { continue };
        if (own.width, own.height) == (framed.width, framed.height) && own == framed {
            continue;
        }
        let id = sheet::frame_style(doc, *page);
        let which = format!(
            "Page {} is {}, but its frame {} is {}; the frame wins.",
            ecad_model::document::book::page_number(doc, *page),
            crate::title_blocks::sheet_name(&own),
            ecad_model::registry::styles::name(doc, crate::styles::catalogue(sheet::FRAME_CATALOGUE), &id),
            crate::title_blocks::sheet_name(&framed)
        );
        out.push(Check {
            kind: Kind::FrameSheetDiffers,
            page: Some(*page),
            at: None,
            object: Object::None,
            which,
            fix: Some(Fix::ClearPageSheet(*page)),
        });
    }
    out
}

/// The input of `page`, from `base` (an input of a page with the same frame and sheet) with the
/// page's own values.
fn page_input(doc: &Document, book: &Book, base: &FrameInput, page: PageId) -> FrameInput {
    let mut inp = base.clone();
    inp.page_title = doc.pages().get(&page).map(|p| p.title.to_string()).unwrap_or_default();
    inp.page = book.number(doc, page);
    inp.next_page = book.next(page);
    inp.next = inp.next_page.map_or_else(|| "-".to_string(), |n| book.number(doc, n));
    // Stored as entry keys of the project's list; shown as codes.
    // The page's own id, else the project's.
    let shown = |a: crate::locations::designations::Aspect| {
        crate::locations::designations::inherited_key(doc, page, a)
            .map(|k| crate::locations::designations::display(doc, a.property(), &k))
            .unwrap_or_default()
    };
    inp.function = shown(crate::locations::designations::Aspect::Function);
    inp.location = shown(crate::locations::designations::Aspect::Location);
    inp
}

/// The fields of `inp`'s block whose value is cut, by item index.
pub(crate) fn cut_fields(inp: &FrameInput) -> Vec<usize> {
    let l = &inp.block.layout;
    let (usable, _) = l.usable();
    usable
        .into_iter()
        .filter(|i| match l.items.get(*i) {
            Some(BlockItem::Field { at, bind, draw, if_empty, .. }) => {
                let v = crate::title_blocks::bind::shown(crate::title_blocks::bind::value(inp, bind), if_empty);
                let (shown, _) = crate::pages::frames::fit(&v, draw.size, Some(at.w - 2.0 * crate::title_blocks::INSET_MM));
                !v.is_empty() && shown.ends_with('…') && !v.ends_with('…')
            }
            _ => false,
        })
        .collect()
}

/// `title-value-cut`: one row per frame and field cut on at least one page.
fn cut_rows(doc: &Document) -> Vec<Check> {
    let book = Book::of(doc);
    let mut bases: BTreeMap<(String, String), FrameInput> = BTreeMap::new();
    // (frame, item) → (first page, pages, field name)
    let mut cut: BTreeMap<(String, usize), (PageId, usize, String)> = BTreeMap::new();
    for page in book.order.iter().take(MAX_PAGES) {
        let id = sheet::title_block_style(doc, *page);
        // One input per frame, borrowed block and sheet.
        let key = (format!("{} {}", sheet::frame_style(doc, *page).text(), id.text()), sheet::sheet_of(doc, *page).text());
        let base = bases.entry(key).or_insert_with(|| FrameInput::with(doc, *page, &book, crate::pages::frames::FrameText::sizes(doc)));
        let mut inp = page_input(doc, &book, base, *page);
        inp.total = base.total.clone();
        for i in cut_fields(&inp) {
            let name = match inp.block.layout.items.get(i) {
                Some(BlockItem::Field { bind, label, .. }) => {
                    label.clone().filter(|l| !l.is_empty()).unwrap_or_else(|| crate::title_blocks::bind::name(bind))
                }
                _ => continue,
            };
            let e = cut.entry((id.text(), i)).or_insert((*page, 0, name));
            e.1 += 1;
        }
    }
    cut.into_iter()
        .map(|((_, i), (page, n, name))| {
            let which = format!("{name} is cut in the title block on {}.", count(n, "page", "pages"));
            let item = u16::try_from(i).unwrap_or(u16::MAX);
            Check { kind: Kind::TitleValueCut, page: Some(page), at: None, object: Object::None, which, fix: Some(Fix::EditTitleBlock(page, item)) }
        })
        .collect()
}
