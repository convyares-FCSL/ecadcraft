//! ecad-drawing-style: the look of an ECAD page, drawn on the definitions of `ecad-model` and the
//! electrical rules of `ecad-electrical`.
//!
//! The look is the text styles and the project font ([`text`]), the text, location-box and frame
//! style catalogues ([`catalogues`]), the title block a frame draws ([`title_blocks`]), and their
//! built-in values ([`defaults`]). [`text::fonts::FontLook`] answers the rules'
//! [`ecad_model::look::Look`] with them, and [`REGISTRY`] holds every crate's defaults.
//!
//! This crate **excludes** any toolkit type and the screen's own look (shell colours, spacing, UI type and
//! icons are `ecad-widgets`' tokens); the canvas palette is `ecad_model::palette`. What a value
//! means electrically, and what fills a title block's fields, stays in `ecad-electrical`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use ecad_model::registry::data::Registry;

pub mod catalogues;
pub mod defaults;
pub mod text;
pub mod title_blocks;

/// The whole registry: every crate's defaults and style catalogues, as one list.
pub static REGISTRY: Registry = Registry::new(&[&ecad_electrical::defaults::data::DATA, &defaults::data::DATA]);
