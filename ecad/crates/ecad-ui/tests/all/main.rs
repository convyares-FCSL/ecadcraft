//! `ecad-ui` tests (one executable, brief 017's layout). Headless: no renderer, no wgpu.

// Lanes in flight add their modules in `lanes/`, not to the list below (see `lanes/mod.rs`).
mod lanes;

mod common;
mod fonts_embedded;
mod icons;
mod primitives;
mod style;
