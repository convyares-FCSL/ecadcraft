//! ecad-drawing-style: the look of an ECAD page, drawn on the definitions of `ecad-model` and the
//! electrical rules of `ecad-electrical`.
//!
//! This crate **excludes** any toolkit type and the screen's own look (shell colours, spacing, UI type and
//! icons are `ecad-widgets`' tokens); the canvas palette is `ecad_model::palette`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use ecad_model::registry::data::Registry;

/// The whole registry: every crate's defaults and style catalogues, as one list.
pub static REGISTRY: Registry = Registry::new(&[&ecad_electrical::defaults::data::DATA]);
