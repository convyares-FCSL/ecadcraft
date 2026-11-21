//! The Pages list and tree rows, and which ops change the tree.

use super::*;

/// A page and, when it is the active page, the engine document showing it (the project's one
/// page window, brief 016: exactly one page is shown and editable at a time).
#[derive(Debug, Clone, Default)]
pub struct PageView {
    pub uid: Option<u64>,
    pub map: HandleMap,
    /// The drawing the owner last wrote (or accepted) for this page.
    pub last: Option<Arc<Drawing>>,
    /// Projection and rebuild both failed: the page must be closed and reopened (PROJ-02).
    pub unavailable: bool,
}

/// One page as the Project panel lists it ([`Project::page_rows`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRow {
    pub id: PageId,
    /// The number the page is shown with ([`crate::references::page_number`]).
    pub number: String,
    pub title: String,
    /// Its projection failed (PROJ-02).
    pub unavailable: bool,
    /// Its folder depth (0 at the top level) and folder (brief 019).
    pub depth: usize,
    pub folder: Option<ecad_model::FolderId>,
    /// Another page is shown with the same number (only from old data).
    pub duplicate: bool,
}

/// One row of the Pages tree (design 019 §5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeRow {
    Folder {
        id: ecad_model::FolderId,
        name: String,
        /// 1 at the top level.
        depth: usize,
        /// Pages inside, all levels.
        pages: usize,
        parent: Option<ecad_model::FolderId>,
    },
    Page(PageRow),
}

/// The Pages tree of `doc` (see [`Project::tree_rows`]); `unavailable` says which pages failed.
pub fn tree_rows(doc: &Document, unavailable: impl Fn(PageId) -> bool) -> Vec<TreeRow> {
    use ecad_model::book::{Book, Node};
    let book = Book::of(doc);
    let numbers: Vec<(PageId, String)> = book.order.iter().map(|p| (*p, book.number(doc, *p))).collect();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (_, n) in &numbers {
        *seen.entry(n.to_ascii_lowercase()).or_default() += 1;
    }
    // Pages per folder, all levels: count each page up its folder chain.
    let mut counts: HashMap<ecad_model::FolderId, usize> = HashMap::new();
    for p in &book.order {
        let mut at = ecad_model::book::folder_of(doc, *p).filter(|f| doc.folders.contains_key(f));
        let mut steps = 0;
        while let Some(f) = at {
            *counts.entry(f).or_default() += 1;
            at = doc.folders.get(&f).and_then(|x| x.parent);
            steps += 1;
            if steps > ecad_model::book::MAX_FOLDER_DEPTH {
                break;
            }
        }
    }
    let number: HashMap<PageId, &String> = numbers.iter().map(|(p, n)| (*p, n)).collect();
    book.nodes
        .iter()
        .map(|n| match n {
            Node::Folder { id, depth } => TreeRow::Folder {
                id: *id,
                name: doc.folders.get(id).map(|f| f.name.clone()).unwrap_or_default(),
                depth: *depth,
                pages: counts.get(id).copied().unwrap_or(0),
                parent: doc.folders.get(id).and_then(|f| f.parent),
            },
            Node::Page { id, depth, position } => {
                let num = number.get(id).map(|s| s.to_string()).unwrap_or_else(|| position.to_string());
                TreeRow::Page(PageRow {
                    id: *id,
                    duplicate: seen.get(&num.to_ascii_lowercase()).is_some_and(|c| *c > 1),
                    number: num,
                    title: doc.pages.get(id).map(|p| p.title.to_string()).unwrap_or_default(),
                    unavailable: unavailable(*id),
                    depth: *depth,
                    folder: ecad_model::book::folder_of(doc, *id).filter(|f| doc.folders.contains_key(f)),
                })
            }
        })
        .collect()
}
