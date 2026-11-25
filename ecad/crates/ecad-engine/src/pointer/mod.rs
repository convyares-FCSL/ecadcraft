//! The pointer over a page: what is under it, what the running tool would do there, and what the
//! click-based forms of the commands share.
//!
//! # Semantics
//! - Each click-based form is CADCraft's prompt machine (`Interactive`) on the command's
//!   `CommandSpec`, in its feature's `interact` module. It only collects and resolves values;
//!   every commit runs the command's **JSON form** through `Session::execute` with those values,
//!   so it records exactly the transaction the JSON form records (one per placed object). The
//!   machine never edits the drawing itself.
//! - A machine's `input` runs while the session has taken the command out of `running`, so the
//!   JSON form commits there exactly as at `Command:`; the engine's commit at the end of the
//!   command then finds nothing new. Esc keeps what was committed and drops the pending input.
//! - A connection point is *captured* when the cursor is within the APERTURE (in pixels) of one
//!   shown on the page ([`pick::capture`]).
//!
//! This module adds no ECAD meaning: no new transaction kinds, nothing stored. It contains no UI
//! toolkit code.

pub mod hover;
pub mod intent;
pub mod kind;
pub mod marks;
pub mod pick;
pub(crate) mod preview;
