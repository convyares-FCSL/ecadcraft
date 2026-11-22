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

pub mod connectivity;
mod dev;
pub mod document;
pub mod error;
pub mod geometry;
pub mod merge;
pub mod naming;
pub mod objects;

pub use crate::connectivity::{Connectivity, NetData, NetKey, Node};
pub use crate::document::book::PageNo;
pub use crate::document::doc::Properties;
pub use crate::document::doc::cable::{Cable, Core, CoreLabel, CorePosition, MAX_CORE_LABEL, Purpose};
pub use crate::document::doc::definition::{Definition, Join, NotASlotText, SlotText, SymbolDefault, SymbolDefaultError, SymbolDefaults, TextSlot};
pub use crate::document::doc::device::{Accepts, ConnectionPoint, ConnectionPointDef, ConnectionPointOwner, Device};
pub use crate::document::doc::document::Document;
pub use crate::document::doc::drawing::{
    Color, Folder, Form, Item, ItemKind, Layer, Page, Primitive, Representation, Style, Subject, SymbolInstance, TextHAlign, TextVAlign,
};
pub use crate::document::doc::settings::{ContactMirror, ContentHash, LengthUnit, Origin, ProjectSettings, ReferenceStyle};
pub use crate::document::invariant::Touched;
pub use crate::document::ledger::Ledger;
pub use crate::document::ledger::types::{Author, Base, BranchPoint, Check, History, StepPoint, Transaction, TxKind};
pub use crate::document::op::{ObjectRef, Op};
pub use crate::document::page::{GenKind, PageKind, PageTitle};
pub use crate::document::project::{
    CustomField, Function, InfoField, ListCode, ListDescription, ListEntry, Location, Project, ProjectInfo, RevField, RevValue, Revision,
};
pub use crate::document::rev::{PointKind, PointName, Rev, RevError, RevPrefix, RevScheme, VersionName};
pub use crate::document::sheet::{GridLabels, PageColumn, PageRow, SheetSpec};
pub use crate::error::{ModelError, Noun, ParseError, Parsed, Result};
pub use crate::geometry::geom::{Placement, Pt};
pub use crate::geometry::point::{ChannelRole, LevelIo, PointKey};
pub use crate::naming::id::{
    AidId, CableId, ConductorId, ConnectionPointId, DefinitionId, DeviceId, FolderId, FunctionId, IdAllocator, ItemId, LayerId, LibraryId, LinkId,
    LocationId, LoosePartId, NetId, PageId, ProjectId, RepresentationId, RouteId, StandardId, TxId, UserFieldId, WireId,
};
pub use crate::naming::words::{Side, Word};
pub use crate::objects::aid::{Aid, AidKind, LocationBox};
pub use crate::objects::devices::{DeviceGroup, DeviceType, TextField};
pub use crate::objects::role::{PartRole, Role};
pub use crate::objects::route::{Route, RouteEnd};
pub use crate::objects::styles::{Standard, StandardHeader, StyleCatalogue, StyleCatalogues, StyleLayer, StyleRecord, StyleRef};
pub use crate::objects::user::{DeviceLink, LinkKind, UserFieldDef, UserFieldKind, UserValue};
pub use crate::objects::wire::{End, Wire};

/// Version of the op and document schema. Independent of the product version (proposal §8.1):
/// it changes only when the meaning or shape of recorded ops changes.
pub const SCHEMA_VERSION: u32 = 1;
