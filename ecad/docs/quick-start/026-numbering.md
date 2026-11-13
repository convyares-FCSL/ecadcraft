# Brief 026: numbering

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 026 (design `ecad/docs/design/026.md`), on `feature/numbering`, not yet in `dev`:** one
numbering mechanism for devices, cables and wires. **Renumber…** (ECAD tab, new **Numbering**
group; the Devices tab's header; a folder's right-click **Renumber in this folder…**; **Renumber
Wires** opens it with Wires chosen) previews old → new in **drawing order** (page, then column,
then row of the sheet grid) and applies it as one transaction (one `u`). Typing a tag fixes it
(**Fixed** beside Tag releases it); a taken tag offers **Swap** first, then **Make it the same
device**, then **Use -K08**. **Project settings → Numbering → Provisional tags until Finalise**
gives new devices `-K?1`, cables `-W?1` and new nets `24V-?` (orange) until **Finalise
numbering**; Checks lists them and Issue Revision asks **Finalise numbering now** or **Issue
anyway**. New cables are tagged `-W01`… automatically; devices and cables share one tag namespace.
A merge's tag conflict offers **Make them one device**. Section 5M describes it.
- *Automated checks:* `ecad/crates/ecad-model/src/numbering.rs` (drawing order, scopes, fixed,
  provisional, hostile requests), `tag.rs` (provisional form, the high-water reset), `merge.rs`
  (Make them one, provisional duplicates), `ecad/apps/ecad-mcp/tests/all/numbering_contracts.rs`
  (every new or changed write through `execute`, its transaction and author, writes off), and
  `ecad/crates/ecad-cadcraft/tests/all/numbering_contracts.rs` (labels, Checks, save and reopen).
- *Agent-observed:* the worker looked at the headless screenshots `026-*.png`; no agent has
  clicked through the live window.
- *Owner-tried:* not yet.
