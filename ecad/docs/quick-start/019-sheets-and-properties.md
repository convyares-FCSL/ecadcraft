# Brief 019: sheets and properties

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 019 (design `ecad/docs/design/019.md`), on `feature/sheets` (built on
`feature/project-manager`), not yet in `dev`:** every page is an A3 sheet with a lettered and
numbered grid and a title block; references read `page-column-row` (`105-C-4`); pages have their
own numbers and sit in folders; project and page properties are edited in CADCraft's
**Properties** palette and a **Project Properties** dialog; revisions are issued and opened
read-only. Section 5H describes it; 5D, 5E and 5G changed with it (texts such as `/2.1` now read
`2-A-6`, and moving a page no longer renumbers it).
- *Automated checks:* `ecad/crates/ecad-cadcraft/tests/all/sheets_contracts.rs` (design 019 checks
  A–G and J), `ecad/crates/ecad-model/tests/all/sheet_contracts.rs` (folders and their
  invariants), the round trip in `ecad/crates/ecad-file/tests/all/records_contracts.rs`, check I in
  `ecad/apps/ecad-mcp/tests/all/mcp_contracts.rs`, and
  `ecad/apps/ecad-trial/tests/all/sheets_ui.rs` (the tree, the Properties section, both dialogs,
  Rev A and the undo question, by the UI in a headless 1280×800 window, with the eight `019-*.png`
  screenshots).
- *Agent-observed:* the worker rendered the `019-*.png` screenshots headless (wgpu) and looked at
  them; no agent has clicked through the live window.
- *Owner-tried:* not yet.
