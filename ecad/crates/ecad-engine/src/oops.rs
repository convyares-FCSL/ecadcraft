//! OOPS on ECAD pages (brief 023, overseer decision 2026-11-09).
//!
//! CADCraft 0.17.0 added OOPS: it brings back what the drawing's last ERASE, BLOCK or -WBLOCK
//! removed, with the objects' old handles and no provenance. On an ECAD page that is safe for plain
//! drawing, which comes back as the same plain items. It is not safe for anything ECAD gave a
//! meaning: a wire's route or a component's line would come back as a loose line, and a symbol,
//! tag text or computed part cannot come back as what it was. Those are refused, with a reason
//! (UNDO brings them back), never silently.
//!
//! The adapter cannot read the engine's OOPS stash, so it remembers, per page document, what the
//! last command that fills that stash removed ([`Record`]): only plain items on one page, or
//! anything else. OOPS is refused before it starts when that record is not plain drawing on the
//! active page, and its commit is refused when it brings back anything that record does not list.
//! UNDO and REDO make the record stale (the drawing changed under it), and so does drawing
//! another page into the same window, so OOPS after them is refused too.
//!
//! This module excludes: any change to the engine's stash (that would be a seam); MINSERT and
//! -ARRAY, which the commit gate already records or refuses (copies with provenance, references
//! to ECAD blocks refused).

use std::collections::{BTreeSet, HashMap};

use cadcraft_engine::doc::{Drawing, Handle};
use ecad_model::{Document, Op, PageId};

use crate::project::HandleMap;

/// Commands that replace the engine's OOPS stash when they remove objects.
pub const STASH_COMMANDS: &[&str] = &["erase", "erase.selection", "block", "-block", "wblock", "-wblock"];

/// Why OOPS is refused on an ECAD page.
pub const REFUSED: &str = "OOPS on ECAD pages brings back only plain drawing removed by the last erase on this page; use UNDO to bring back \
                           wires, devices and other objects ECAD records";

/// What the last stash-filling command removed on one page document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    /// Only plain items (no representation) on `page`, by their handles.
    Plain { page: PageId, handles: BTreeSet<Handle> },
    /// Anything else, or a stale record (refused commit, UNDO, REDO).
    Refused,
}

/// The records of every page document of a project, by the engine document's uid.
#[derive(Clone, Debug, Default)]
pub struct Records(HashMap<u64, Record>);

impl Records {
    /// Whether OOPS may start on document `uid` showing `page`.
    ///
    /// # Semantics
    /// With no record (nothing erased since the page opened) OOPS has nothing to bring back and
    /// may run; with a plain record it may run on the same page only.
    pub fn refuse_start(&self, uid: u64, page: PageId) -> Option<String> {
        match self.0.get(&uid) {
            None => None,
            Some(Record::Plain { page: p, .. }) if *p == page => None,
            Some(_) => Some(REFUSED.into()),
        }
    }

    /// Whether an OOPS commit that turned `last` into `now` brings back only what the record lists.
    ///
    /// # Semantics
    /// Every object in `now` that is not in `last` must be one of the plain handles recorded for
    /// `page`. Looks at the whole drawing, once per OOPS (OOPS is rare and changes no other view).
    pub fn allows(&self, uid: u64, page: PageId, last: &Drawing, now: &Drawing) -> bool {
        let Some(Record::Plain { page: p, handles }) = self.0.get(&uid) else { return false };
        if *p != page {
            return false;
        }
        let before: BTreeSet<Handle> = last.model.iter().map(|e| e.handle).collect();
        now.model.iter().map(|e| e.handle).filter(|h| !before.contains(h)).all(|h| handles.contains(&h))
    }

    /// Remember what an accepted stash-filling command removed: `ops` as translated against `doc`
    /// (the document before the commit) and `map` (the page's handles before it).
    ///
    /// # Semantics
    /// Plain only when every op removes an item without a representation; anything else (a
    /// representation unlinked, a device deleted, a terminal removed…) is [`Record::Refused`].
    pub fn note_accepted(&mut self, uid: u64, page: PageId, doc: &Document, map: &HandleMap, ops: &[Op]) {
        let mut handles = BTreeSet::new();
        let plain = ops.iter().all(|op| match op {
            Op::RemoveItem { id, .. } if doc.representation_of_item(*id).is_none() => map.item(*id).map(|h| handles.insert(h)).is_some(),
            _ => false,
        });
        self.0.insert(uid, if plain { Record::Plain { page, handles } } else { Record::Refused });
    }

    /// A stash-filling command on `uid` was refused: what the engine kept is not plain drawing.
    pub fn note_refused(&mut self, uid: u64) {
        self.0.insert(uid, Record::Refused);
    }

    /// UNDO or REDO changed the pages: every record is stale.
    pub fn stale(&mut self) {
        for r in self.0.values_mut() {
            *r = Record::Refused;
        }
    }

    /// Another page was drawn into document `uid`: what OOPS kept is not on it.
    pub fn stale_doc(&mut self, uid: u64) {
        if let Some(r) = self.0.get_mut(&uid) {
            *r = Record::Refused;
        }
    }

    /// The page document `uid` went away.
    pub fn forget(&mut self, uid: u64) {
        self.0.remove(&uid);
    }
}
