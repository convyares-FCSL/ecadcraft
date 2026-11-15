//! The engine boundary (design 065 §3, cut 11): the CADCraft engine types and functions that the
//! App and the Programs (ecad-app, ecad-desktop, ecad-mcp) may name. They reach the engine only through this
//! list, never through `cadcraft_engine` directly.
//!
//! # Semantics
//! Plain re-exports; nothing here wraps or changes the engine. A new engine type the App needs is
//! added here on purpose, in review.
//!
//! Excluded: everything else of `cadcraft_engine` (drawing internals, commands' own types,
//! rendering), and anything ECAD-specific.

pub use cadcraft_engine::doc::{Drawing, Handle, Space, entity_bounds, extent_bounds};
pub use cadcraft_engine::fonts::ttf::installed as installed_fonts;
pub use cadcraft_engine::geom::{Bounds2, Vec2};
pub use cadcraft_engine::{Input, Session, Settings, View, find_command};
