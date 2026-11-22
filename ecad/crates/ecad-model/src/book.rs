//! Book order and page numbers (brief 019, design 019 §5.1, §6.1).
//!
//! # Semantics
//! - A page's folder is its [`crate::Page::folder`]; `None` means the top level. Folders nest through [`crate::Folder::parent`].
//! - **Book order**: within the top level and within each folder, pages come first (in
//!   `page_order` order), then sub-folders (in `folder_order` order), read depth-first (owner
//!   answer Q6). With no folders it is `page_order`. A page whose folder cannot be reached (only
//!   from a hostile document) comes last, in `page_order` order, so every page has a place.
//! - A page's **shown number** is its [`crate::Page::number`], else its 1-based
//!   position in book order (old files, brief AC6).
//!
//! Building a [`Book`] costs one pass over the pages and folders, never over their items.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use crate::doc::Document;
use crate::id::{FolderId, PageId};
use crate::text::Text;

/// The old page property naming the page's folder; now [`crate::Page::folder`] (a reserved key).
pub const FOLDER_KEY: &str = "folder";
/// The old page property of the page number; now [`crate::Page::number`] (a reserved key).
pub const NUMBER_KEY: &str = "number";
/// Page and project property: the `=` function (design 019 §6.1, 018 value rules).
pub const FUNCTION_KEY: &str = "function";
/// Page and project property: the `+` location.
pub const LOCATION_KEY: &str = "location";
/// `setprop` key of a device's [`crate::Device::location_fixed`] (yes/no).
pub const LOCATION_FIXED_KEY: &str = "location_fixed";
/// `setprop` key of a device's [`crate::Device::function_fixed`] (yes/no).
pub const FUNCTION_FIXED_KEY: &str = "function_fixed";

/// Folders nest at most this deep (a top-level folder is depth 1).
pub const MAX_FOLDER_DEPTH: usize = 8;

/// The folder `page` is in, by its `folder` property, when that reads as a folder ID.
pub fn folder_of(doc: &Document, page: PageId) -> Option<FolderId> {
    doc.pages.get(&page).and_then(|p| p.folder)
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

/// What book order depends on, and nothing else: the page order, each page's folder, the folders
/// and their order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outline {
    pub page_order: Vec<PageId>,
    /// Each page's folder, when its `folder` property names an existing folder.
    pub page_folder: BTreeMap<PageId, FolderId>,
    /// Each folder's parent.
    pub folders: BTreeMap<FolderId, Option<FolderId>>,
    pub folder_order: Vec<FolderId>,
}

impl Outline {
    /// The outline of `doc`: O(pages + folders).
    pub fn of(doc: &Document) -> Outline {
        Outline {
            page_order: doc.page_order.clone(),
            page_folder: doc.page_order.iter().filter_map(|p| folder_of(doc, *p).map(|f| (*p, f))).collect(),
            folders: doc.folders.iter().map(|(id, f)| (*id, f.parent)).collect(),
            folder_order: doc.folder_order.clone(),
        }
    }

    /// The book order of this outline.
    pub fn book(&self) -> Book {
        // Children per parent (None = top level), each list in its stored order.
        let mut pages_in: BTreeMap<Option<FolderId>, Vec<PageId>> = BTreeMap::new();
        for p in &self.page_order {
            let f = self.page_folder.get(p).copied().filter(|f| self.folders.contains_key(f));
            pages_in.entry(f).or_default().push(*p);
        }
        let mut folders_in: BTreeMap<Option<FolderId>, Vec<FolderId>> = BTreeMap::new();
        for f in &self.folder_order {
            let parent = self.folders.get(f).map(|x| x.filter(|p| self.folders.contains_key(p)));
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
        for p in &self.page_order {
            if !book.positions.contains_key(p) {
                book.push_page(*p, 0);
            }
        }
        book
    }
}

/// The pages whose book position a commit changes, found by comparing the book before with the
/// book after (a full build each side, O(pages + folders); no update map since 0.2.3): the
/// positions of the state before are kept at the first effect that can move a page
/// ([`moves_pages_by_one`]), so a commit that moves no page reads no book.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PositionsNoted {
    before: Option<BTreeMap<PageId, usize>>,
}

impl PositionsNoted {
    /// Before phase: note one effect, read on `before` (the state before the commit).
    pub fn note_before(&mut self, before: &Document, e: &crate::effect::Effect<'_>) {
        if self.before.is_none() && moves_pages_by_one(e) {
            self.before = Some(Book::of(before).positions);
        }
    }

    /// After phase: the pages of the state before whose book position differs in `after`.
    pub fn changed(&self, after: &Document) -> BTreeSet<PageId> {
        let Some(before) = &self.before else { return BTreeSet::new() };
        let now = Book::of(after).positions;
        before.iter().filter(|(p, i)| now.get(p) != Some(i)).map(|(p, _)| *p).collect()
    }
}

/// Whether `effects` can change book order at all: a page added, removed or moved, a page's folder
/// changed, a folder effect.
pub fn moves_pages_by(effects: &crate::effect::Effects<'_>) -> bool {
    effects.iter().any(moves_pages_by_one)
}

/// Whether one effect can move a page in the book (one effect of [`moves_pages_by`]).
pub fn moves_pages_by_one(e: &crate::effect::Effect<'_>) -> bool {
    use crate::effect::Effect;
    match e {
        Effect::Page { before: Some(b), after: Some(a), index_before: None, index_after: None, .. } => b.folder != a.folder,
        Effect::Page { .. } | Effect::Folder { .. } => true,
        _ => false,
    }
}

impl Book {
    /// The book order of `doc`.
    pub fn of(doc: &Document) -> Book {
        Outline::of(doc).book()
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
        stored_number(doc, page).map_or_else(|| self.position(page).map_or_else(|| "?".to_string(), |i| i.to_string()), str::to_string)
    }
}

/// `page`'s stored number, if it has one.
pub fn stored_number(doc: &Document, page: PageId) -> Option<&str> {
    doc.pages.get(&page).and_then(|p| p.number.as_ref()).map(PageNo::as_str)
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

/// Most characters of a page number.
pub const MAX_PAGE_NO_CHARS: usize = 12;

/// The number a page is shown with (`100`, `105a`, `A_01`): 1 to [`MAX_PAGE_NO_CHARS`] ASCII
/// letters, digits or `_` (design 072 §9.3).
///
/// # Semantics
/// Built only through [`PageNo::parse`] or deserialisation, which refuse any other text, so a
/// held `PageNo` is always well formed. Case is kept as written.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct PageNo(Text<MAX_PAGE_NO_CHARS>);

impl PageNo {
    /// The page number `text` is, if it is well formed. Surrounding whitespace is ignored.
    ///
    /// # Example
    /// ```
    /// use ecad_model::book::PageNo;
    /// assert_eq!(PageNo::parse(" 105a ").map(|n| n.to_string()), Some("105a".into()));
    /// assert!(PageNo::parse("1.5").is_none());
    /// ```
    pub fn parse(text: &str) -> Option<PageNo> {
        let t = Text::new(text).ok()?;
        t.as_str().bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_').then_some(PageNo(t))
    }

    /// The number's text.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for PageNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_str())
    }
}

impl<'de> Deserialize<'de> for PageNo {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        PageNo::parse(&text)
            .filter(|n| n.as_str() == text)
            .ok_or_else(|| serde::de::Error::custom(format!("`{text}` is not a page number (1-{MAX_PAGE_NO_CHARS} letters, digits or _)")))
    }
}
