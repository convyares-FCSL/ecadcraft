//! ecad-electrical
//!
//! The electrical rules of an ECAD project, decided from the document alone (design 065 §1, Core):
//! devices, wires, cables, terminal strips, connectors and accessories; cross-references,
//! continuations and link lines; project checks and proposed fixes; the rating dataset; the
//! reports (bill of materials, device list, outputs, PLC lists) and the layout of every generated
//! page, title block and frame; page types, locations, revisions, styles and project defaults; and
//! the shared vocabulary (symbols, representations, kinds, words, designations, text styles).
//!
//! It works on [`ecad_model`]'s document and the drawing style ([`ecad_drawing_style`]). What it
//! decides is plain data: positions in mm, texts, styles and model ids. The engine crate
//! (`ecad-engine`) turns that into drawing entities and runs it inside commands.
//!
//! This crate intentionally contains **no** drawing engine (no `cadcraft-*` crate, no entities or
//! handles), **no** UI, **no** OS access (no files, clock, environment or process: callers pass the
//! time in) and **no** font lookup: text is measured through
//! [`crate::styles::text_style::fonts::TextMetrics`], which the caller supplies.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub use crate::error::{CmdError, msg};

pub mod cables;
pub mod checks;
pub mod circuits;
pub mod commands;
pub mod connectors;
pub mod continuations;
pub mod defaults;
pub mod devices;
pub mod error;
pub mod locations;
pub mod merge;
pub mod numbering;
pub mod pages;
pub mod plc;
pub mod properties;
pub mod references;
pub mod releases;
pub mod reports;
pub mod styles;
pub mod symbols;
pub mod terminal_strips;
pub mod title_blocks;
pub mod views;
pub mod wires;

