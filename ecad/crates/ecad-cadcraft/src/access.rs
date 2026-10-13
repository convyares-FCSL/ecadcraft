//! The lock boundary between an ECAD project and the CADCraft session.
//!
//! # Ownership and re-entry contract
//! A [`Project`] is the owner of its page documents. The engine locks it (with `try_lock`) while it
//! runs one of the project's [`DocumentOwner`](cadcraft_engine::ext::DocumentOwner) callbacks; the
//! adapter locks it while an ECAD command or a caller reads it. Calling an owner-aware session
//! method (`execute`, `cmdline`, `undo`, `redo`, `doc_unsaved`, `close_needs_prompt`,
//! `has_unsaved_work`, save or close) while the project is locked cannot succeed: the engine finds
//! the owner busy and refuses the call or answers conservatively ("unsaved"). This module makes
//! that mistake impossible to write instead of documenting it:
//!
//! - Outside this crate a project is only reachable through a [`ProjectHandle`]. Its accessors lock
//!   with `try_lock` (a busy project is an error, never a deadlock) and take the session as
//!   `&mut Session` for the length of the call, so the closure cannot capture the session: the
//!   borrow checker rejects any session call made while the project is locked. The session passed
//!   must be the one showing the project's pages, so the proof cannot be faked with another one.
//! - Inside this crate, project code never sees a `Session`. It works through [`Pages`] and
//!   [`PagesRef`], which expose only the document operations a projection needs (find, replace and
//!   open page drawings, bind them, echo) and none of the owner-aware methods.
//!
//! ```compile_fail,E0502
//! # use cadcraft_engine::Session;
//! # fn f(project: &ecad_cadcraft::ProjectHandle, s: &mut Session) {
//! // Rejected: the session is borrowed by `read` while the closure runs.
//! project.read(s, |p, _| s.doc_unsaved(0) == p.ledger.is_unsaved());
//! # }
//! ```
//!
//! ```
//! # use cadcraft_engine::Session;
//! # fn f(project: &ecad_cadcraft::ProjectHandle, s: &mut Session) -> Result<bool, String> {
//! // Take what is needed from the project, release it, then ask the session.
//! let unsaved = project.read(s, |p, _| p.ledger.is_unsaved())?;
//! Ok(s.doc_unsaved(0) == unsaved)
//! # }
//! ```

use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use cadcraft_engine::doc::{Drawing, Entity, Space};
use cadcraft_engine::ext::OwnerRef;
use cadcraft_engine::{DocState, Session};
use ecad_model::PageId;
use ecad_store::MemoryMedium;
use serde_json::Value;

use crate::project::Project;

/// Message for a project that is already locked further up the call stack.
pub const BUSY: &str = "the ECAD project is busy (re-entrant call)";
/// Message for a session that does not hold the project's open pages.
pub const NOT_ATTACHED: &str = "the ECAD project is not attached to this session";

/// Lock without blocking. A lock poisoned by a caught panic is still usable (the engine asked the
/// project to recover, or the panic happened before any change).
pub(crate) fn try_lock<T: ?Sized>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    match m.try_lock() {
        Ok(g) => Ok(g),
        Err(TryLockError::Poisoned(p)) => Ok(p.into_inner()),
        Err(TryLockError::WouldBlock) => Err(BUSY.into()),
    }
}

/// A project attached to a session: the only way to reach a [`Project`] from outside this crate.
///
/// # Semantics
/// Every accessor takes `&mut Session` and holds the project lock only for its own length; see
/// the module documentation for the contract. Cloning the handle shares the project.
#[derive(Clone)]
pub struct ProjectHandle(Arc<Mutex<Project>>);

impl ProjectHandle {
    pub(crate) fn new(p: Arc<Mutex<Project>>) -> Self {
        Self(p)
    }

    /// Lock for a call that borrows `s`. The borrow only proves something if `s` is the session
    /// that can call this project back, so a project with open pages must be bound in `s`.
    fn lock(&self, s: &Session) -> Result<MutexGuard<'_, Project>, String> {
        let p = try_lock(&self.0)?;
        let me = Arc::as_ptr(&self.0).cast::<()>();
        if p.open_pages() > 0 && !s.ext.owners().iter().any(|o| Arc::as_ptr(o).cast::<()>() == me) {
            return Err(NOT_ATTACHED.into());
        }
        Ok(p)
    }

    /// Read the project and the page drawings.
    ///
    /// # Errors
    /// [`BUSY`] when the project is already locked (a call from inside a project callback);
    /// [`NOT_ATTACHED`] when `s` is not the session showing the project's pages.
    pub fn read<R>(&self, s: &mut Session, f: impl FnOnce(&Project, &PagesRef<'_>) -> R) -> Result<R, String> {
        let p = self.lock(s)?;
        Ok(f(&p, &PagesRef(s)))
    }

    /// Change project settings that are not part of the design (fault injection, the test medium,
    /// the snapping tolerance). Design changes go through commands, never through this.
    ///
    /// # Errors
    /// [`BUSY`] or [`NOT_ATTACHED`], as for [`ProjectHandle::read`].
    pub fn configure<R>(&self, s: &mut Session, f: impl FnOnce(&mut Project) -> R) -> Result<R, String> {
        let mut p = self.lock(s)?;
        Ok(f(&mut p))
    }

    /// Open `page` as a document (or switch to it).
    ///
    /// # Errors
    /// [`BUSY`], an unknown page, or projection failure.
    pub fn open_page(&self, s: &mut Session, page: PageId) -> Result<u64, String> {
        self.lock(s)?.open_page(&mut Pages(s), page)
    }

    /// A JSON inspection snapshot (live when `at` is `None`).
    ///
    /// # Errors
    /// [`BUSY`] or see [`ecad_inspect::snapshot`].
    pub fn snapshot(&self, s: &mut Session, at: Option<u64>) -> Result<Value, String> {
        self.lock(s)?.snapshot(&PagesRef(s), at)
    }

    /// A debugger snapshot, unless `known` is still current (`Ok(None)`): everything the panel
    /// shows, copied under one short lock; `have` is the end of the caller's copy of the history
    /// (see [`crate::debug`]).
    ///
    /// # Errors
    /// [`BUSY`] or [`NOT_ATTACHED`], as for [`ProjectHandle::read`]. A busy project is never
    /// waited for: the caller keeps its previous snapshot.
    pub fn debug_snapshot(
        &self,
        s: &mut Session,
        known: Option<&crate::debug::SnapshotKey>,
        have: crate::debug::TxCursor,
    ) -> Result<Option<crate::debug::DebugSnapshot>, String> {
        let p = self.lock(s)?;
        let pg = PagesRef(s);
        let key = p.debug_key(&pg);
        if known == Some(&key) {
            return Ok(None);
        }
        Ok(Some(p.debug_snapshot(&pg, key, have)))
    }

    /// Copies of the document and of every open page's drawing and handle map, for
    /// [`crate::debug::verify_projection`] (which then runs with the project unlocked).
    ///
    /// # Errors
    /// [`BUSY`] or [`NOT_ATTACHED`].
    pub fn debug_projection_inputs(&self, s: &mut Session) -> Result<crate::debug::ProjectionInputs, String> {
        let p = self.lock(s)?;
        Ok(p.projection_inputs(&PagesRef(s)))
    }

    /// Reopen a project from an in-memory project file; see [`Project::open_from`].
    ///
    /// # Errors
    /// Store or replay failure.
    pub fn open_from(s: &mut Session, name: &str, medium: MemoryMedium, seed: u64) -> Result<Self, String> {
        Project::open_from(s, name, medium, seed)
    }

    /// Open a project file from disk; see [`Project::open_file`]. Returns the handle and what
    /// opening found, in words.
    ///
    /// # Errors
    /// The file cannot be read or is not a supported project file.
    pub fn open_file(s: &mut Session, path: &str, seed: u64) -> Result<(Self, String), String> {
        Project::open_file(s, path, seed)
    }
}

/// The project that owns the active document, for ECAD commands.
pub(crate) fn owner_of_active(s: &Session) -> Result<OwnerRef, String> {
    let uid = s.state().map_err(|e| e.to_string())?.uid;
    s.ext.owner_of(uid).ok_or_else(|| "this is not an ECAD page".into())
}

/// Page documents as project code may change them. Wraps the session and exposes none of its
/// owner-aware methods.
pub(crate) struct Pages<'a>(pub(crate) &'a mut Session);

impl Pages<'_> {
    /// The open document `uid`.
    pub(crate) fn doc(&self, uid: u64) -> Option<&DocState> {
        self.0.docs.iter().find(|d| d.uid == uid)
    }

    /// The open document `uid`, to replace its drawing, selection, `saved` pointer or path.
    pub(crate) fn doc_mut(&mut self, uid: u64) -> Option<&mut DocState> {
        self.0.docs.iter_mut().find(|d| d.uid == uid)
    }

    /// Make document `uid` the active one; `false` when it is not open.
    pub(crate) fn activate(&mut self, uid: u64) -> bool {
        match self.0.docs.iter().position(|d| d.uid == uid) {
            Some(i) => {
                self.0.active = i;
                true
            }
            None => false,
        }
    }

    /// Open `d` as a new document bound to `owner`; returns its uid.
    pub(crate) fn open(&mut self, d: Drawing, title: &str, path: Option<String>, owner: OwnerRef) -> Option<u64> {
        let i = self.0.open_drawing(d, title, path);
        let uid = self.0.docs.get(i)?.uid;
        self.0.ext.bind(uid, owner);
        Some(uid)
    }

    /// Whether an interactive command is running (its drawing holds uncommitted work).
    pub(crate) fn command_running(&self) -> bool {
        self.0.running.is_some()
    }

    /// Show a message at the command line.
    pub(crate) fn echo(&mut self, m: impl Into<String>) {
        self.0.echo(m);
    }

    pub(crate) fn as_ref(&self) -> PagesRef<'_> {
        PagesRef(self.0)
    }
}

/// Page documents, read only.
pub struct PagesRef<'a>(&'a Session);

impl PagesRef<'_> {
    /// The open document `uid`.
    pub fn doc(&self, uid: u64) -> Option<&DocState> {
        self.0.docs.iter().find(|d| d.uid == uid)
    }

    /// The drawing of the engine document showing `page` of `p`.
    pub fn drawing_of(&self, p: &Project, page: PageId) -> Option<Arc<Drawing>> {
        let uid = p.pages.get(&page)?.uid?;
        self.doc(uid).map(|d| d.doc.clone())
    }

    /// Model-space entities of `page`'s drawing.
    pub fn entities_of(&self, p: &Project, page: PageId) -> Vec<Arc<Entity>> {
        self.drawing_of(p, page)
            .and_then(|d| d.space(&Space::Model).map(|m| m.iter().cloned().collect()))
            .unwrap_or_default()
    }

    /// The active document's uid and selection.
    pub(crate) fn active(&self) -> Option<(u64, Vec<cadcraft_engine::doc::Handle>)> {
        self.0.state().ok().map(|st| (st.uid, st.selection.clone()))
    }

    /// Index of document `uid` among the open documents.
    pub fn index_of(&self, uid: u64) -> Option<usize> {
        self.0.docs.iter().position(|d| d.uid == uid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The project held further up the stack (as during an owner callback): ECAD commands and
    /// handle accessors are refused at once instead of blocking forever.
    #[test]
    fn a_held_project_refuses_instead_of_deadlocking() {
        crate::install();
        let mut s = Session::empty();
        let handle = Project::create(&mut s, "P", 1).unwrap();
        let held = handle.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let r = s.execute("wire", &serde_json::json!({"points": [[0, 0], [10, 0]]}));
        assert!(r.is_err_and(|e| e.to_string().contains(BUSY)));
        assert_eq!(handle.read(&mut s, |_, _| ()).err().as_deref(), Some(BUSY));
        assert_eq!(handle.snapshot(&mut s, None).err().as_deref(), Some(BUSY));
        drop(held);
        assert!(handle.read(&mut s, |p, _| p.ledger.head()).is_ok());
        assert!(s.execute("wire", &serde_json::json!({"points": [[0, 0], [10, 0]]})).is_ok());
    }
}
