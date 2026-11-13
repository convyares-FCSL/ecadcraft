# Brief 022: connectivity and wire numbers

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 022 (design `ecad/docs/design/022.md`), on `feature/connectivity`, not yet in `dev`:**
ECAD knows what is connected. Nets come from the attachments (a T joins only at a junction dot,
crossings never join, continuations with one name join across pages); every wire shows its net's
wire number with its cross-section and colour (`24V-04 0.5 BU`); adding a wire never renumbers
another net; the Project panel has a **Checks** tab. Section 5I describes it, on the demo project.
- *Automated checks:* `ecad/crates/ecad-model/tests/all/connectivity_contracts.rs` (net rules,
  numbering rules, a random-op test against a fresh build after every step),
  `ecad/crates/ecad-cadcraft/tests/all/wiring_contracts.rs` (T, crossing, labels, stable numbers,
  one row per seeded fault, the kept Checks list against a fresh build),
  `ecad/apps/ecad-mcp/tests/all/demo.rs` (the demo's numbers) and `mcp_contracts.rs` (every new
  command through MCP), `ecad/apps/ecad-trial/tests/all/checks_ui.rs` (the Checks tab by label,
  go-to, Add junction, and the `022-*.png` screenshots).
- *Agent-observed:* the worker rendered the `022-*.png` screenshots headless (wgpu) and looked at
  them; no agent has clicked through the live window.
- *Owner-tried:* not yet.
