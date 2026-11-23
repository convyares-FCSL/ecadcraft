//! `ecad-widgets` tests (one test executable). Headless: no renderer, no wgpu.

// Lanes in flight add their modules in `lanes/`, not to the list below (see `lanes/mod.rs`).
mod common;
mod fonts_embedded;
mod icons;
mod primitives;
mod style;
mod table;
mod tokens;
mod wheel;
