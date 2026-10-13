//! Embedder extensions: documents with an owner (ECAD-SEAM(owner)).
//!
//! An embedding application may give a document an *owner*: an object that keeps its own
//! authoritative model and treats the drawing as a view of it. The engine then reports to the owner
//! at a few well-defined moments and lets it decide:
//!
//! - **commit**: after a command changed (or did not change) an owned drawing, and after the
//!   post-command passes, the owner accepts the result (possibly after amending the drawing
//!   itself) or refuses it (and restores the drawing). No undo snapshot is pushed for owned
//!   documents: the owner keeps the history.
//! - **undo / redo**: [`crate::Session::undo`] and [`crate::Session::redo`] hand over to the owner.
//! - **save / close**: QSAVE and SAVEAS hand over; closing asks whether a prompt is needed and tells
//!   the owner the document went away.
//! - **provenance**: copy-like operations record which new object came from which source, and a
//!   clipboard copy is reported, so the owner can carry its own meaning across a copy.
//! - **drag preview**: the owner may add rubber-band geometry to the running command's preview.
//!
//! Plain drawings have no owner and behave exactly as before. Nothing here is specific to any
//! embedder: the interface passes drawings, handles, command ids and provenance only.

use std::any::Any;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use cadcraft_doc::{EntityKind, Handle};
use serde_json::Value;

use crate::prompt::Drag;
use crate::{EngineError, Result, Session};

/// Where a newly created entity came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// An entity of the same drawing (COPY, MIRROR/ROTATE/SCALE with copy).
    Entity(Handle),
    /// An entity on the clipboard, by its handle in the drawing it was copied from.
    Clipboard { serial: u64, handle: Handle },
}

/// One provenance record: `new` was created from `source` by the `batch`-th copy operation of the
/// command (COPY with several placements creates one batch per placement).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub new: Handle,
    pub source: Source,
    pub batch: u32,
}

/// A clipboard copy made during the command: its serial, the document copied from and the handles
/// copied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardCopy {
    pub serial: u64,
    pub uid: u64,
    pub handles: Vec<Handle>,
}

/// What the engine tells the owner when a command commits.
#[derive(Debug)]
pub struct Commit<'a> {
    /// The owned document.
    pub uid: u64,
    /// The command's id.
    pub command: &'a str,
    pub provenance: &'a [Provenance],
    pub clipboard: Option<&'a ClipboardCopy>,
}

/// The owner's answer to a commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The drawing (as it is now, possibly amended by the owner) is accepted.
    Accept,
    /// Refused; the owner has already restored the drawing. The message is shown to the user.
    Refuse(String),
}

/// An object that owns documents. Implemented by the embedder.
///
/// # Semantics
/// Callbacks run on the engine's thread with the session borrowed mutably, so the owner may read
/// and replace the drawings it owns. A callback that panics is caught; the engine then calls
/// [`DocumentOwner::recover`] and reports an error.
///
/// # Ownership and re-entry
/// The engine holds the owner's lock (`try_lock`, never a blocking lock) for the length of each
/// callback. A callback that calls back into the engine for the same owner is therefore refused,
/// never deadlocked, and the engine keeps the drawing consistent:
/// - a command (`execute`, `cmdline`) is refused with "busy (re-entrant call)" and the engine
///   restores the drawing it started from, because the busy owner cannot;
/// - `undo`, `redo`, save and close are refused with the same error; a refused close keeps the
///   document open and bound;
/// - state queries (`doc_unsaved`, `close_needs_prompt`, `has_unsaved_work`) answer
///   conservatively ("unsaved", "ask first").
///
/// The embedder's own lock is its business: an embedder that holds its owner's lock while it calls
/// the engine gets the same refusals, so embedders should expose an API that releases the lock
/// before calling the engine (ECAD's adapter does).
pub trait DocumentOwner: Send {
    /// A command committed on owned document `c.uid`.
    fn commit(&mut self, s: &mut Session, c: &Commit<'_>) -> Verdict;
    /// UNDO on an owned document. Returns the label undone, `None` when there is nothing to undo.
    fn undo(&mut self, s: &mut Session, uid: u64) -> Result<Option<String>>;
    /// REDO on an owned document.
    fn redo(&mut self, s: &mut Session, uid: u64) -> Result<Option<String>>;
    /// QSAVE (`save_as` false) or SAVEAS on an owned document; `path` as given by the user.
    fn save(&mut self, s: &mut Session, uid: u64, path: Option<&str>, save_as: bool) -> Result<Value>;
    /// Whether the owner holds work that is not saved.
    fn is_unsaved(&self) -> bool;
    /// Whether closing document `uid` must ask "Save changes?" first.
    fn close_needs_prompt(&self, uid: u64) -> bool;
    /// Document `uid` is being closed. Return `true` when the owner has nothing left open and can
    /// be removed.
    fn closed(&mut self, s: &mut Session, uid: u64) -> bool;
    /// Extra rubber-band geometry while the running command drags objects of document `uid`.
    fn drag_extra(&self, _s: &Session, _uid: u64, _drag: &Drag) -> Vec<EntityKind> {
        Vec::new()
    }
    /// A callback panicked: bring every owned drawing back to a consistent state.
    fn recover(&mut self, s: &mut Session);
    /// Name shown in prompts.
    fn label(&self) -> String;
    /// For the embedder's own commands to reach their state.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// A shared owner.
pub type OwnerRef = Arc<Mutex<dyn DocumentOwner>>;

/// The session's owners and what the current command has recorded for them.
#[derive(Default)]
pub struct Extensions {
    owners: Vec<OwnerRef>,
    bound: HashMap<u64, OwnerRef>,
    provenance: Vec<Provenance>,
    batch: u32,
    clipboard: Option<ClipboardCopy>,
}

impl std::fmt::Debug for Extensions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Extensions").field("owners", &self.owners.len()).field("bound", &self.bound.len()).finish()
    }
}

impl Extensions {
    /// Register an owner.
    pub fn register(&mut self, owner: OwnerRef) {
        if !self.owners.iter().any(|o| Arc::ptr_eq(o, &owner)) {
            self.owners.push(owner);
        }
    }
    /// Make `owner` the owner of document `uid` (registering it if needed).
    pub fn bind(&mut self, uid: u64, owner: OwnerRef) {
        self.register(owner.clone());
        self.bound.insert(uid, owner);
    }
    /// The owner of document `uid`.
    pub fn owner_of(&self, uid: u64) -> Option<OwnerRef> {
        self.bound.get(&uid).cloned()
    }
    /// Every registered owner.
    pub fn owners(&self) -> &[OwnerRef] {
        &self.owners
    }
    /// Remove an owner and its bindings.
    pub fn remove(&mut self, owner: &OwnerRef) {
        self.owners.retain(|o| !Arc::ptr_eq(o, owner));
        self.bound.retain(|_, o| !Arc::ptr_eq(o, owner));
    }
    /// Whether any owner is registered (recording is skipped otherwise).
    pub fn active(&self) -> bool {
        !self.owners.is_empty()
    }
    /// A top-level command starts: forget what an earlier command recorded.
    pub(crate) fn begin_command(&mut self) {
        self.provenance.clear();
        self.clipboard = None;
    }
    /// Record copies made by one copy operation.
    pub(crate) fn note_copies(&mut self, pairs: impl IntoIterator<Item = (Handle, Source)>) {
        if !self.active() {
            return;
        }
        self.batch = self.batch.wrapping_add(1);
        let batch = self.batch;
        self.provenance.extend(pairs.into_iter().map(|(new, source)| Provenance { new, source, batch }));
    }
    /// Record a clipboard copy.
    pub(crate) fn note_clipboard(&mut self, copy: ClipboardCopy) {
        if self.active() {
            self.clipboard = Some(copy);
        }
    }
}

/// Lock an owner. `None` when it is already locked on this thread (a re-entrant call from inside
/// one of its callbacks). A lock poisoned by a caught panic is still usable: the owner was asked to
/// recover.
fn lock(owner: &OwnerRef) -> Option<MutexGuard<'_, dyn DocumentOwner + 'static>> {
    match owner.try_lock() {
        Ok(g) => Some(g),
        Err(TryLockError::Poisoned(p)) => Some(p.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

const REENTRANT: &str = "the document owner is busy (re-entrant call)";

fn recover(s: &mut Session, owner: &OwnerRef) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if let Some(mut o) = lock(owner) {
            o.recover(s);
        }
    }));
}

/// Outcome of the commit gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Gate {
    /// Not an owned document: the engine's own undo applies.
    NotOwned,
    /// Owned and accepted: no engine undo step.
    Accepted,
    /// Owned and refused (the drawing was restored).
    Refused(String),
}

/// Ask the owner of document `uid` to accept what `command` did (`before` is the drawing it started from).
pub(crate) fn gate(s: &mut Session, uid: u64, command: &str, before: &Arc<cadcraft_doc::Drawing>) -> Gate {
    let Some(owner) = s.ext.owner_of(uid) else {
        s.ext.begin_command();
        return Gate::NotOwned;
    };
    let provenance = std::mem::take(&mut s.ext.provenance);
    let clipboard = s.ext.clipboard.take();
    let c = Commit { uid, command, provenance: &provenance, clipboard: clipboard.as_ref() };
    match catch_unwind(AssertUnwindSafe(|| lock(&owner).map(|mut o| o.commit(s, &c)))) {
        Ok(Some(Verdict::Accept)) => Gate::Accepted,
        Ok(Some(Verdict::Refuse(m))) => Gate::Refused(m),
        Ok(None) => {
            // The owner is busy and cannot restore its drawing, so the engine undoes the change.
            if let Some(st) = s.docs.iter_mut().find(|d| d.uid == uid) {
                st.doc = before.clone();
            }
            Gate::Refused(REENTRANT.into())
        }
        Err(_) => {
            recover(s, &owner);
            Gate::Refused(format!("internal error while recording `{command}`; the drawing was rebuilt from the document"))
        }
    }
}

/// UNDO (`undo` true) or REDO on the active document, when it has an owner.
pub(crate) fn history(s: &mut Session, undo: bool) -> Option<Result<Option<String>>> {
    let uid = s.state().ok()?.uid;
    let owner = s.ext.owner_of(uid)?;
    // The owner may replace several drawings; every changed one gets a new revision so caches
    // keyed by (uid, revision) refresh, as they do after the engine's own undo.
    let before: Vec<(u64, Arc<cadcraft_doc::Drawing>, u64)> = s.docs.iter().map(|d| (d.uid, d.doc.clone(), d.revision)).collect();
    let r = catch_unwind(AssertUnwindSafe(|| match lock(&owner) {
        Some(mut o) if undo => o.undo(s, uid),
        Some(mut o) => o.redo(s, uid),
        None => Err(EngineError::Other(REENTRANT.into())),
    }));
    let r = r.unwrap_or_else(|_| {
        recover(s, &owner);
        Err(EngineError::Internal(if undo { "undo" } else { "redo" }.into(), "the document owner failed".into()))
    });
    for d in &mut s.docs {
        if let Some((_, doc, rev)) = before.iter().find(|(u, _, _)| *u == d.uid)
            && !Arc::ptr_eq(doc, &d.doc)
            && d.revision == *rev
        {
            d.revision += 1;
        }
    }
    Some(r)
}

/// QSAVE or SAVEAS on the active document, when it has an owner.
pub(crate) fn save(s: &mut Session, path: Option<&str>, save_as: bool) -> Option<Result<Value>> {
    let uid = s.state().ok()?.uid;
    let owner = s.ext.owner_of(uid)?;
    let r = catch_unwind(AssertUnwindSafe(|| match lock(&owner) {
        Some(mut o) => o.save(s, uid, path, save_as),
        None => Err(EngineError::Other(REENTRANT.into())),
    }));
    Some(r.unwrap_or_else(|_| {
        recover(s, &owner);
        Err(EngineError::Internal("save".into(), "the document owner failed".into()))
    }))
}

/// Document `uid` is about to be closed. On failure (the owner is busy or its callback panics)
/// the close must not proceed: the owner is asked to recover and the document stays open and bound.
pub(crate) fn closing(s: &mut Session, uid: u64) -> Result<()> {
    let Some(owner) = s.ext.owner_of(uid) else { return Ok(()) };
    let done = match catch_unwind(AssertUnwindSafe(|| lock(&owner).map(|mut o| o.closed(s, uid)))) {
        Ok(Some(done)) => done,
        Ok(None) => return Err(EngineError::Other(REENTRANT.into())),
        Err(_) => {
            recover(s, &owner);
            return Err(EngineError::Internal("close".into(), "the document owner failed; the document stays open".into()));
        }
    };
    s.ext.bound.remove(&uid);
    if done {
        s.ext.remove(&owner);
    }
    Ok(())
}

/// Extra drag preview geometry from the owner of the active document.
pub(crate) fn drag_extra(s: &Session, drag: &Drag) -> Vec<EntityKind> {
    let Some(uid) = s.state().ok().map(|st| st.uid) else { return Vec::new() };
    let Some(owner) = s.ext.owner_of(uid) else { return Vec::new() };
    catch_unwind(AssertUnwindSafe(|| lock(&owner).map(|o| o.drag_extra(s, uid, drag)).unwrap_or_default())).unwrap_or_default()
}

/// Ask an owner a yes/no question about its state. A busy owner (asked from inside one of its
/// callbacks) or one whose answer panics gives `true`: "unsaved" and "ask first" are the safe
/// answers, so no work is lost by a faulty owner.
fn ask(owner: &OwnerRef, q: impl FnOnce(&dyn DocumentOwner) -> bool) -> bool {
    catch_unwind(AssertUnwindSafe(|| lock(owner).is_none_or(|o| q(&*o)))).unwrap_or(true)
}

impl Session {
    /// Whether document `i` has unsaved work: for an owned document, its owner's answer.
    pub fn doc_unsaved(&self, i: usize) -> bool {
        let Some(d) = self.docs.get(i) else { return false };
        match self.ext.owner_of(d.uid) {
            Some(o) => ask(&o, |o| o.is_unsaved()),
            None => d.is_dirty(),
        }
    }

    /// Whether closing document `i` must ask "Save changes?" first.
    pub fn close_needs_prompt(&self, i: usize) -> bool {
        let Some(d) = self.docs.get(i) else { return false };
        match self.ext.owner_of(d.uid) {
            Some(o) => ask(&o, |o| o.close_needs_prompt(d.uid)),
            None => d.is_dirty(),
        }
    }

    /// Whether any open document or owner holds unsaved work (quit asks first).
    pub fn has_unsaved_work(&self) -> bool {
        self.docs.iter().any(|d| self.ext.owner_of(d.uid).is_none() && d.is_dirty()) || self.ext.owners.iter().any(|o| ask(o, |o| o.is_unsaved()))
    }
}
