# Release 0.2.6: UI improvement

Status: Draft, not started (the full list is still to come)
Release ladder: [0.2.4 tidy-up](release-0.2.4.md) → [0.2.5 features](release-0.2.5.md) → 0.2.6 UI →
[0.2.7 format release](release-0.2.7.md) → 0.3.0 ([roadmap](../roadmap.md)).

## Goal

The function works; this release makes the interface good to use.

## Items

1. **BOM rating columns.** They work, but need a lot of interface work. Seen in use: ratings are set only
   through a device's Properties on the drawing, and the Parts list preview gives no hint where; the Columns ▸
   Rating submenu lists every rating field of every group, even ones no device in the project has. More
   specifics to come.
2. **Overlaps on drawn pages.** The wire label `N-01 2.5 BU` sits over the second `-X01` tag; continuation
   references `2/C1 3/D1` run into the revision block's Date column. Fixed in 0.2.4 if it is a layout bug,
   here if it is styling.
3. **Reports as a top-level mode** beside Draw, Project, Library and Publish. Project's tabs (Devices,
   Terminals, PLC, Wires, Cables) manage project-wide things; reports are output (sheets, Excel and CSV), not
   management. Reports are read-only: a rating, part or value is edited in Draw or Devices, never in a
   report, and a report cell that is missing data points to where to set it.
4. **Report tables:** column widths and row heights adjustable and the spacing tighter, as the cable views do
   it.
5. **A home screen when no project is open:** recent projects to pick from (name, path, last opened, and a
   page thumbnail if cheap), New, and Open from disk; Close returns to it. It replaces the empty window, and
   Open or New from inside a project becomes close, then home.
6. **Paged reports:** each report sheet a page of its own with its own title block, updated per sheet. Brief
   and design: [058-paged-reports.md](058-paged-reports.md) and its design, written against the tree before
   0.2.3, so re-check paths and the design before use. Pairs with item 3.
7. **Open a project on the background pool with a progress state (B19):** opening runs on the UI thread
   today. Fits the home screen (item 5).
8. **Where the interface is going.** The target layout also has Network, Power and Tests tabs in the Project
   workspace, a panel layout page type with its own tool strip, and the Library's Snippets and Parts tabs
   filled in. Where these are not built the shell shows them disabled with the reason, or not at all. The
   features behind them are placed in the [roadmap](../roadmap.md); this release gives their place in the
   shell its final form.
