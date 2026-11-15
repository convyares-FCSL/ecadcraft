# Brief 060: tidy-up guards and the check-kinds table

Part of the [quick start](../quick-start.md).

## What this brief changed

Branch `chore/tidy-guards` (brief 060). It is a tidy-up: the file format is unchanged and nothing
outside `ecad/` changed. Nothing changes that you can click: the one change in behaviour is
reached through the `setprop` command (an agent through ecad-mcp, or a typed command line).

- **`setprop` on several devices at once checks the value as it would on one.** Before,
  `setprop {components: [...], key, value}` stored values that the same edit on one device
  refuses: a connector's `pins` of `abc`, a PLC address of `x1`, a strip's level separator longer
  than 4 characters, a level label over 24 characters.
  Now the edit is refused for all of them, with the same message as for one.
- The Checks list still shows the same rows, names and words. The kinds now come from one table
  in `checks.rs`, and the four whole-project row sources (styles, cover, designations, title
  blocks) each emit their rows from their own module.
- `cargo xtask layers` also checks:
  - egui's named colours (`Color32::WHITE`, `GRAY`, `RED`, `YELLOW`) count as local styling;
  - literal sizes (`Stroke::new(1.0`, `add_space(4.0)`, `vec2(12.0`) in UI files are counted, and
    the counts can only fall;
  - the apps' direct use of `cadcraft-engine` is counted, and the count can only fall;
  - `anyhow` is allowed only in the apps and xtask, and `tokio` in no ECAD crate;
  - direct file system, environment, process, network and clock access in `ecad-model` and
    `ecad-file` is counted per file, and the counts can only fall.

## Try it

There is no click-only step: the Trial does not set a property on several devices at once yet.
To see it, give ecad-mcp's `execute` (or the command line) `setprop` with two connectors' IDs in
`components`, key `pins` and value `abc`: it is refused and both stay at 4 pins; with `6`, both
have 6 pins.

Automated checks: `cargo test -p ecad-cadcraft lane_060`, `cargo test -p ecad-model
incremental_contracts`, `cargo test -p ecad-xtask`, `cargo xtask layers`. Nobody has tried this
outside the tests yet.
