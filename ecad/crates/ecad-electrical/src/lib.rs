//! ecad-electrical: the electrical rules of an ECAD project, decided from the document alone.
//!
//! It reads [`ecad_model`]'s document and returns plain data
//! (positions in mm, texts, styles, model ids) or ops; `ecad-engine` turns that into drawing entities and runs it
//! inside commands. One folder per feature area:
//!
//! - [`cables`]: the cable planning model, cable slash and cable layout.
//! - [`checks`]: the Checks list (open ends, loose wires, single-point nets, number and potential conflicts).
//! - [`circuits`]: wire and cable sizing (Ib ≤ In ≤ Iz, volt drop), each result a Checks row.
//! - [`commands`]: the generic command runner and the typed command input shared by every area.
//! - [`connectors`]: plug and socket pairs drawn as one mating device.
//! - [`continuations`]: continuation arrows and where their other ends are.
//! - [`defaults`]: the defaults registry, one entry per project default and its levels.
//! - [`devices`]: devices and tags, device kinds, ratings, the device list and placing.
//! - [`error`]: the crate's error type and the refusals said in more than one place.
//! - [`input`]: the grammars of text typed into a field, each refusing with a named error.
//! - [`locations`]: device locations and functions, location boxes and designations.
//! - [`merge`]: what a value typed in a merge conflict is stored as.
//! - [`numbering`]: renumbering, fixed and provisional tags.
//! - [`pages`]: page types, folders, frames, generated pages and the cover.
//! - [`plc`]: PLC racks, cards and channels, and the PLC lists and sheets.
//! - [`properties`]: the ECAD section of the Properties palette and the edits its rows write.
//! - [`references`]: device and cable references.
//! - [`releases`]: revisions, issue, release and the pre-release check.
//! - [`reports`]: the report models: BOM, wire report and cable report.
//! - [`styles`]: style catalogues and text styles.
//! - [`symbols`]: the starter symbols, the symbol builder and symbol offers.
//! - [`terminal_strips`]: terminal strips, terminal blocks, jumper bars, accessories and cable cores.
//! - [`title_blocks`]: title blocks: frame parts, field values, the house block and DXF import.
//! - [`views`]: the one update pipeline for every derived view.
//! - [`wires`]: wires on the page: conductor labels, routes, joins and continuation texts.
//!
//! It contains **no** drawing engine (no `cadcraft-*` crate, no entities or handles), **no** UI, **no** OS access
//! (no files, clock, environment or process: callers pass the time in) and **no** font lookup: text is measured
//! through [`crate::styles::text_style::fonts::TextMetrics`], which the caller supplies.
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
pub mod input;
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
