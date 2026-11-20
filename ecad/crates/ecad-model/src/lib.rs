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
//! - [`device`]: what a device is, its [`DeviceType`] (design 072 §2).
//! - [`derive`]: results recomputed from the document (terminal positions, connection mismatches).
//! - [`connectivity`]: nets, kept incrementally by every op (brief 022).
//! - [`wire`]: the wire and its typed values (design 072 §4.2).
//! - [`wiring`]: wire numbers, potentials and conductor labels (brief 022).
//! - [`words`], [`units`] and [`keys`]: the shared word readers, the unit types and the property
//!   key list (design 072 §9); [`text`]: validated text; [`colour`]: conductor colours.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod book;
pub mod builtins;
pub mod colour;
pub mod connectivity;
pub mod derive;
mod dev;
pub mod device;
pub mod diff;
pub mod doc;
pub mod effect;
pub mod error;
pub mod geom;
pub mod id;
mod index;
pub mod invariant;
pub mod keys;
pub mod ledger;
pub mod links;
pub mod merge;
pub mod numbering;
pub mod op;
pub mod page;
pub mod plc;
pub mod point;
pub mod prefix;
pub mod project;
pub mod route;
pub mod sheet;
pub mod strict;
pub mod style;
pub mod tag;
pub mod text;
pub mod title_block;
pub mod units;
pub mod view;
pub mod wire;
pub mod wire_order;
pub mod wiring;
pub mod words;

pub use book::PageNo;
pub use connectivity::{Connectivity, NetData, NetKey, Node};
pub use device::{DeviceGroup, DeviceType, TextField};
pub use doc::{
    Accepts, Cable, Color, ContactMirror, ContentHash, Core, CoreLabel, CorePosition, DEFINITION_DEFAULT_KEYS, Definition, Device, Document, End,
    Folder, Form, Item, ItemKind, Layer, LengthUnit, MAX_CORE_LABEL, Origin, Page, Primitive, ProjectSettings, Properties, Purpose, ReferenceStyle,
    Representation, Style, Subject, SymbolInstance, Terminal, TerminalDef, TextHAlign, TextSlot, TextVAlign, Wire,
};
pub use error::{ModelError, Noun, ParseError, Parsed, Result};
pub use geom::{Placement, Pt};
pub use id::{
    CableId, ConductorId, DefinitionId, DeviceId, FolderId, FunctionId, IdAllocator, ItemId, LayerId, LibraryId, LocationId, NetId, PageId,
    ProjectId, RepresentationId, RouteId, StandardId, TerminalId, TxId, WireId,
};
pub use invariant::Touched;
pub use ledger::{Author, Base, BranchPoint, Check, History, Ledger, StepPoint, Transaction, TxKind};
pub use op::{ObjectRef, Op};
pub use page::{GenKind, PageKind, PageTitle};
pub use point::{ChannelRole, LevelIo, PointKey};
pub use project::{
    CustomField, Function, InfoField, ListCode, ListDescription, ListEntry, Location, Project, ProjectInfo, Rev, RevField, RevValue, Revision,
};
pub use route::{Route, RouteEnd};
pub use sheet::{GridLabels, PageColumn, PageRow, SheetSpec};
pub use style::{Standard, StandardHeader, StyleCatalogue, StyleCatalogues, StyleLayer, StyleRecord, StyleRef};
pub use words::{Side, Word};

/// Version of the op and document schema. Independent of the product version (proposal §8.1):
/// it changes only when the meaning or shape of recorded ops changes.
pub const SCHEMA_VERSION: u32 = 1;
