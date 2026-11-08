//! Book order and page numbers (brief 019, design 019 §5.1, §6.1).
//!
//! # Semantics
//! - A page's folder is its `folder` property ([`FOLDER_KEY`]): a folder's 32-hex-digit ID; absent
//!   means the top level. Folders nest through [`crate::Folder::parent`].
//! - **Book order**: within the top level and within each folder, pages come first (in
//!   `page_order` order), then sub-folders (in `folder_order` order), read depth-first (owner
//!   answer Q6). With no folders it is `page_order`. A page whose folder cannot be reached (only
//!   from a hostile document) comes last, in `page_order` order, so every page has a place.
//! - A page's **shown number** is its `number` property ([`NUMBER_KEY`]), else its 1-based
//!   position in book order (old files, brief AC6).
//!
//! Building a [`Book`] costs one pass over the pages and folders, never over their items.

use std::collections::{BTreeMap, BTreeSet};

use crate::doc::Document;
use crate::id::{FolderId, PageId};

/// The page property naming the page's folder.
pub const FOLDER_KEY: &str = "folder";
/// The page property holding the designer's page number (`100`, `105a`).
pub const NUMBER_KEY: &str = "number";
/// Page and project property: the `=` function (design 019 §6.1, 018 value rules).
pub const FUNCTION_KEY: &str = "function";
/// Page and project property: the `+` location.
pub const LOCATION_KEY: &str = "location";

/// Folders nest at most this deep (a top-level folder is depth 1).
pub const MAX_FOLDER_DEPTH: usize = 8;

/// The folder `page` is in, by its `folder` property, when that reads as a folder ID.
pub fn folder_of(doc: &Document, page: PageId) -> Option<FolderId> {
    doc.pages.get(&page).and_then(|p| p.properties.get(FOLDER_KEY)).and_then(|t| FolderId::parse(t))
}

/// One node of the page tree, in book order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Node {
    /// A folder at `depth` (1 for the top level).
    Folder { id: FolderId, depth: usize },
    /// A page at `depth` (0 for the top level), and its 1-based book position.
    Page { id: PageId, depth: usize, position: usize },
}

/// The book order of a document's pages, with the tree it comes from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Book {
    /// Pages in book order.
    pub order: Vec<PageId>,
    /// Each page's 1-based position in [`Book::order`].
    pub positions: BTreeMap<PageId, usize>,
    /// The tree read depth-first: folders and pages in the order the Pages list shows them.
    pub nodes: Vec<Node>,
}

impl Book {
    /// The book order of `doc`.
    pub fn of(doc: &Document) -> Book {
        // Children per parent (None = top level), each list in its stored order.
        let mut pages_in: BTreeMap<Option<FolderId>, Vec<PageId>> = BTreeMap::new();
        for p in &doc.page_order {
            let f = folder_of(doc, *p).filter(|f| doc.folders.contains_key(f));
            pages_in.entry(f).or_default().push(*p);
        }
        let mut folders_in: BTreeMap<Option<FolderId>, Vec<FolderId>> = BTreeMap::new();
        for f in &doc.folder_order {
            let parent = doc.folders.get(f).map(|x| x.parent.filter(|p| doc.folders.contains_key(p)));
            if let Some(parent) = parent {
                folders_in.entry(parent).or_default().push(*f);
            }
        }
        let mut book = Book::default();
        let mut seen: BTreeSet<FolderId> = BTreeSet::new();
        // Depth-first with an explicit stack: (folder, depth); a cycle is never entered twice.
        let mut stack: Vec<(Option<FolderId>, usize)> = vec![(None, 0)];
        while let Some((folder, depth)) = stack.pop() {
            if let Some(f) = folder {
                if !seen.insert(f) {
                    continue;
                }
                book.nodes.push(Node::Folder { id: f, depth });
            }
            for p in pages_in.get(&folder).map(Vec::as_slice).unwrap_or_default() {
                book.push_page(*p, depth);
            }
            let subs = folders_in.get(&folder).map(Vec::as_slice).unwrap_or_default();
            for f in subs.iter().rev() {
                stack.push((Some(*f), depth + 1));
            }
        }
        // Pages in a folder that could not be reached (a cycle, from a hostile document).
        for p in &doc.page_order {
            if !book.positions.contains_key(p) {
                book.push_page(*p, 0);
            }
        }
        book
    }

    fn push_page(&mut self, id: PageId, depth: usize) {
        if self.positions.contains_key(&id) {
            return;
        }
        self.order.push(id);
        let position = self.order.len();
        self.positions.insert(id, position);
        self.nodes.push(Node::Page { id, depth, position });
    }

    /// `page`'s 1-based book position.
    pub fn position(&self, page: PageId) -> Option<usize> {
        self.positions.get(&page).copied()
    }

    /// The page after `page` in book order.
    pub fn next(&self, page: PageId) -> Option<PageId> {
        self.position(page).and_then(|i| self.order.get(i).copied())
    }

    /// The number `page` is shown with: its `number` property, else its book position, else `?`.
    pub fn number(&self, doc: &Document, page: PageId) -> String {
        stored_number(doc, page)
            .map_or_else(|| self.position(page).map_or_else(|| "?".to_string(), |i| i.to_string()), str::to_string)
    }
}

/// `page`'s stored number, if it has one.
pub fn stored_number(doc: &Document, page: PageId) -> Option<&str> {
    doc.pages.get(&page).and_then(|p| p.properties.get(NUMBER_KEY)).map(String::as_str)
}

/// The number `page` is shown with (see [`Book::number`]). Builds the book order only when the
/// page has no stored number.
pub fn page_number(doc: &Document, page: PageId) -> String {
    match stored_number(doc, page) {
        Some(n) => n.to_string(),
        None => Book::of(doc).number(doc, page),
    }
}

/// The depth of `folder` (1 at the top level), walking its parents; `None` when a parent is
/// missing, the parents cycle, or it is deeper than [`MAX_FOLDER_DEPTH`].
pub fn folder_depth(doc: &Document, folder: FolderId) -> Option<usize> {
    let mut depth = 1;
    let mut at = doc.folders.get(&folder)?;
    while let Some(parent) = at.parent {
        depth += 1;
        if depth > MAX_FOLDER_DEPTH {
            return None;
        }
        at = doc.folders.get(&parent)?;
    }
    Some(depth)
}
