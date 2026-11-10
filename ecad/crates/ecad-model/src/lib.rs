//! ecad-model
//!
//! The authoritative ECAD document: objects with stable typed identities, the ops that change
//! them, atomic transactions, and the append-only history that undo and redo extend.
//!
//! This crate intentionally contains **no** drawing engine, UI, file encoding or I/O. It does not
//! depend on any CADCraft crate (principle P10 in `docs/document-architecture-proposal.md`), so an
//! upstream CADCraft change cannot alter the document, its history or replay.
//!
//! Current focus (bounded integration experiment, proposal §17):
//! - [`doc`]: the objects and their relationships, with reverse indexes for queries.
//! - [`invariant`]: the invariants, checked in full or scoped to what a transaction touched.
//! - [`op`]: self-inverting ops that record resolved intent.
//! - [`ledger`]: transactions applied in place with rollback, linear project-wide undo/redo
//!   derived from the log, unsaved state.
//! - [`derive`]: results recomputed from the document (terminal positions, connection mismatches).
//! - [`connectivity`]: nets, kept incrementally by every op (brief 022).
//! - [`wiring`]: wire numbers, potentials and conductor labels (brief 022).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod book;
pub mod connectivity;
pub mod derive;
pub mod diff;
pub mod doc;
pub mod error;
pub mod geom;
pub mod id;
mod index;
pub mod invariant;
pub mod ledger;
pub mod op;
pub mod sheet;
pub mod tag;
pub mod wiring;

pub use connectivity::{Connectivity, NetData, NetId, Node};
pub use doc::{
    Cable, CableMember, Color, Component, Conductor, ContentHash, DEFAULT_PAGE_NUMBER_STEP, DEFAULT_TAG_PADDING, Definition, Document, End, Folder,
    Form, Item, ItemKind, Layer, LengthUnit, Origin, Page, Primitive, ProjectSettings, Properties, ReferenceStyle, Representation, Style, Subject,
    SymbolInstance, Terminal, TerminalDef,
};
pub use error::{ModelError, Result};
pub use geom::{Placement, Pt};
pub use id::{
    CableId, ComponentId, ConductorId, DefinitionId, FolderId, IdAllocator, ItemId, LayerId, LibraryId, PageId, ProjectId, RepresentationId,
    TerminalId, TxId,
};
pub use invariant::Touched;
pub use ledger::{Author, Base, History, Ledger, Transaction, TxKind};
pub use op::{ObjectRef, Op};
pub use sheet::{GridLabels, SheetSpec};

/// Version of the op and document schema. Independent of the product version (proposal §8.1):
/// it changes only when the meaning or shape of recorded ops changes.
pub const SCHEMA_VERSION: u32 = 1;
