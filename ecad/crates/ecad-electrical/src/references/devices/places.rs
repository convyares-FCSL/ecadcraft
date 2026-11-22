//! Page places worked out once per page (grid, shown number, whether it is in the book).

use ecad_model::{Document, PageId, Pt};
use std::collections::BTreeMap;

// ---------------------------------------------------------------- places

/// Page places worked out once per page (grid, shown number, whether it is in the book).
pub struct Places<'a> {
    doc: &'a Document,
    book: Option<ecad_model::document::book::Book>,
    pages: BTreeMap<PageId, (ecad_model::document::sheet::Grid, String, bool)>,
}

impl<'a> Places<'a> {
    pub fn new(doc: &'a Document) -> Self {
        Places { doc, book: None, pages: BTreeMap::new() }
    }

    fn page(&mut self, page: PageId) -> &(ecad_model::document::sheet::Grid, String, bool) {
        let doc = self.doc;
        let book = &mut self.book;
        self.pages.entry(page).or_insert_with(|| {
            // A stored number needs no book order (the usual case): the book is built only for a
            // page shown by its position.
            let number = match ecad_model::document::book::stored_number(doc, page) {
                Some(n) => n.to_string(),
                None => book.get_or_insert_with(|| ecad_model::document::book::Book::of(doc)).number(doc, page),
            };
            (ecad_model::document::sheet::Grid::of(doc, page), number, doc.page_order.contains(&page))
        })
    }

    /// `100-B-3` (or `/100.B`) for `at` on `page`; `?` (`/?`) when it cannot be located.
    pub fn text(&mut self, page: PageId, at: Option<Pt>) -> String {
        let style = self.doc.settings.reference_style();
        let unlocated = ecad_model::connectivity::derive::unlocated(style).to_string();
        let Some(at) = at else { return unlocated };
        let (grid, number, located) = self.page(page);
        if !*located {
            return unlocated;
        }
        ecad_model::connectivity::derive::place_on(grid, number, at).map_or(unlocated, |p| p.text(style))
    }

    /// The page's shown number.
    pub fn number(&mut self, page: PageId) -> String {
        self.page(page).1.clone()
    }

    /// Brief 049 (design 049 §2.2): where `at` on `page` sorts, by book position, then column,
    /// then row, and its text; `None` (shown `?`, sorted last) when it cannot be located. The book
    /// is built at most once per `Places`.
    pub fn sorted(&mut self, page: PageId, at: Option<Pt>) -> Option<((usize, u32, u32), String)> {
        let at = at?;
        let style = self.doc.settings.reference_style();
        let doc = self.doc;
        let position = self.book.get_or_insert_with(|| ecad_model::document::book::Book::of(doc)).position(page)?;
        let (grid, number, located) = self.page(page);
        if !*located {
            return None;
        }
        let p = ecad_model::connectivity::derive::place_on(grid, number, at)?;
        Some(((position, p.column, p.row), p.text(style)))
    }
}
