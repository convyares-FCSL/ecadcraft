//! ecad-inspector
//!
//! ECAD's developer inspector (brief 029). A developer and test tool: it never ships, and no
//! product crate depends on it (`cargo xtask layers`).
//!
//! - [`protocol`]: a versioned JSON protocol (`{"v":1,"id":…,"op":…}`) over a [`source::Source`],
//!   in the manner of rosbridge. [`protocol::handle`] is pure: no I/O, no threads.
//! - [`source`]: a `.ecad` file opened read-only, by path or from dropped bytes, with the chain
//!   check on load and Verify on a background thread.
//! - [`serve`]: `ecadinspect serve`, the page and the protocol over a WebSocket on 127.0.0.1.
//! - [`text`] and [`cli`]: `ecadinspect <file> <view>`, the text views.
//!
//! Excluded: anything that writes a project file; the drawing Checks and derived locations
//! (`ecad-cadcraft`); version 1 files (a message and `ecad upgrade`); attaching to a running app
//! session (a later source).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod cli;
pub mod protocol;
pub mod serve;
pub mod source;
pub mod text;
