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
//! - [`doc`]: the objects and their relationships, with invariant checks.
//! - [`op`]: self-inverting ops that record resolved intent.
//! - [`ledger`]: transactions, linear project-wide undo/redo derived from the log, unsaved state.
//! - [`derive`]: results recomputed from the document (terminal positions, connection mismatches).
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod derive;
pub mod doc;
pub mod error;
pub mod geom;
pub mod id;
pub mod ledger;
pub mod op;

pub use doc::{
    Cable, CableMember, Color, Component, Conductor, Definition, Document, End, Form, Item, ItemKind, Layer, Origin,
    Page, Primitive, Properties, Representation, Style, Subject, SymbolInstance, Terminal, TerminalDef,
};
pub use error::{ModelError, Result};
pub use geom::{Placement, Pt};
pub use id::{
    CableId, ComponentId, ConductorId, DefinitionId, IdAllocator, ItemId, LayerId, PageId, RepresentationId,
    TerminalId, TxId,
};
pub use ledger::{Author, Ledger, Transaction, TxKind};
pub use op::{ObjectRef, Op};

/// Version of the op and document schema. Independent of the product version (proposal §8.1):
/// it changes only when the meaning or shape of recorded ops changes.
pub const SCHEMA_VERSION: u32 = 1;
