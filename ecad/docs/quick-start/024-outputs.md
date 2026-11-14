# Brief 024: outputs

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 024 (design `ecad/docs/design/024.md`), on `feature/outputs`, not yet in `dev`:** a
device's **location** (`+CP1`) is its own value, else the **location box** its main symbol sits
in, else its page's, else the project's; Properties says where it comes from (`+JB1 (from box on
3)`). Location boxes are dashed rectangles or closed polylines labelled `+…`, drawn with
**Location Box** (ECAD tab) or made from an existing closed polyline; what they hold updates live,
and nothing is written on the devices. A symbol drawn away from its device's location shows the
prefix (`+CP1-K01`). **Reports** (BOM, wires, cables, each exported as Excel-safe CSV) and
**Print Book** (one PDF of every page, a folder or a page list) are buttons in the PROJECT panel's
header; its **v** beside **+** adds generated pages: Cover, Contents, Revision list, BOM by
location. Section 5K describes it.
- *Automated checks:* `ecad/crates/ecad-cadcraft/tests/all/outputs_contracts.rs` (precedence, live
  boxes, innermost wins, own value kept and flagged with its fix, the prefix, reports and CSV,
  generated pages and overflow, Print Book; every edit's kept models equal a fresh build),
  `ecad/apps/ecad-mcp/tests/all/outputs_contracts.rs` (MCP writes as the agent, refused with writes
  off; reports and printing read), `ecad/apps/ecad-mcp/tests/all/demo.rs`
  (`the_demo_locations_reports_and_generated_pages`), and `ecad/apps/ecad-trial/tests/all/outputs_ui.rs`
  (the windows, headless, with screenshots).
- *Agent-observed:* the worker looked at the headless screenshots `024-*.png`; no agent has
  clicked through the live window.
