# Brief 028: terminal strips

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 028 (design `ecad/docs/design/028.md`), on `feature/terminal-strips`, not yet in `dev`:**
a terminal clicked on a wire splits it and both halves land on it; **Place Terminals** puts one
terminal on every wire a line crosses; **Draw Bridge** links two terminals of a strip. A strip's
**Terminals offered** (`12+2PE`) gives spare positions; a cable's **Cores** (or its type, `7G1.5`)
gives used, spare and over cores, and a new member takes the next free core. The terminal plan
and the cable schedule are in Reports and on generated pages; the BOM counts a strip per terminal
type; five Checks rows have fixes. Section 5O describes it.
- *Automated checks:* `ecad/crates/ecad-cadcraft/tests/all/strips_contracts.rs` (design 028 §12
  A–H, L, the update map and the FP5 finding; the kept strips equal a rebuild after the edits,
  and in verify mode after every commit), `ecad/apps/ecad-mcp/tests/all/strips_contracts.rs` (the
  brief 011 rule), unit tests of the offer and core grammars in
  `ecad/crates/ecad-cadcraft/src/strips.rs`.
- *Agent-observed:* none yet; no agent has clicked through the live window.
- *Owner-tried:* not yet.
