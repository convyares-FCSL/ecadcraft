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
//! (`ecad-cadcraft`) turns that into drawing entities and runs it inside commands.
//!
//! This crate intentionally contains **no** drawing engine (no `cadcraft-*` crate, no entities or
//! handles), **no** UI, **no** OS access (no files, clock, environment or process: callers pass the
//! time in) and **no** font lookup: text is measured through
//! [`text_style::fonts::TextMetrics`], which the caller supplies.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod accessories;
pub mod bom;
pub mod cables;
pub mod checks;
pub mod connectors;
pub mod continuations;
pub mod defaults;
pub mod designations;
pub mod device_list;
pub mod devices;
pub mod frames;
pub mod front;
pub mod generated;
pub mod kinds;
pub mod link_lines;
pub mod locations;
pub mod outputs;
pub mod page_types;
pub mod plc;
pub mod proposals;
pub mod rating;
pub mod references;
pub mod reps;
pub mod revisions;
pub mod slash;
pub mod strips;
pub mod styles;
pub mod symbols;
pub mod terminal_types;
pub mod text_style;
pub mod title_block;
pub mod wires;
pub mod words;
pub mod xref;
