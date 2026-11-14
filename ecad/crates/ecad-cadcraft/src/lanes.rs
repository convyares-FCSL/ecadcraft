//! The command registrations of lanes in flight (owner, 2026-11-13).
//!
//! A lane (a brief being built on its feature branch) whose new module adds commands registers
//! its `specs` in [`LANES`], one line, instead of adding a line to [`crate::commands::specs`]:
//! lanes then add lines only here, and git merges this file with `merge=union`
//! (`ecad/.gitattributes`), so two lanes adding a line at the same place keep both instead of
//! conflicting. If the merged lines are out of brief order, the `lanes_are_in_brief_order` test
//! says so; swap them. At the phase-end tidy-up the lines move into `commands::specs`, and the list
//! is empty again. Keep this file to the list: union merges are only safe for added lines.

use cadcraft_engine::CommandSpec;

/// The lanes' command lists, appended after the ECAD commands in [`crate::commands::specs`]: one
/// line per lane, `("<brief>", crate::<module>::specs),`, in brief order.
pub(crate) const LANES: &[(&str, fn() -> Vec<CommandSpec>)] = &[
    // ("NNN", crate::<module>::specs), one line per lane, below this one.
];
