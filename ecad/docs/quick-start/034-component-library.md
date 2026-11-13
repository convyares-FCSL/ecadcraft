# Brief 034: component library

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 034 (design `ecad/docs/design/034.md`), on `feature/component-library`, not yet in
`dev`:** the built-in symbols grow from 21 to 81 so a control-panel pack needs no hand-drawn boxes:
change-over contacts (`co`, `swco`: `11`/`12`/`14`, then `21`/`22`/`24`… when joined), 4-pole power
contacts, RCBO and RCDs, SSR input and output, a fuse terminal and fuse banks of 4 to 12 ways;
proximity, float, roller and interlock switches, single and dual solenoid valves, a thermocouple
and a heater; plugs, sockets, D-sub and M12 halves, RJ45 and USB bulkheads; PLC card bodies
(CPU, PS, DI, DQ, AI, AO, RTD, IO-Link). Find Symbol also finds a family word (`protection`,
`sensors`, `connectors`, `plc`…). Any device takes a `rating`; a fuse bank takes `rating.<n>` per
fuse. Spare connector pins, a fuse bank's `M` and outputs and an I/O card's supply points are not
flagged as unconnected. Ratings are stored and listed but not drawn yet (lane T draws text);
contact tables do not count change-over contacts yet (stage 4, after brief 033). Section 5-034.
- *Automated checks:* `ecad/crates/ecad-cadcraft/src/symbols.rs` (every symbol on the grid, its
  hash, box and labels, the change-over rule, quiet points, anchors, search),
  `ecad/crates/ecad-cadcraft/tests/all/library_contracts.rs` (placing, joining, ratings, nets,
  quiet points, BOM rows) and `ecad/apps/ecad-mcp/tests/all/library_contracts.rs` (every new symbol,
  a change-over join and ratings through `execute` as the agent; writes off; hostile input).
- *Agent-observed:* the worker looked at a drawing of every new definition; no agent has clicked
  through the live window.

## 5-034. Component library (by clicking)

Brief 034, design `ecad/docs/design/034.md`. Any project; open the **ECAD** tab first.

1. **Change-over contacts.** Place a new coil. In Find Symbol type `change`, click
   **Change-over contact**, then that coil's device line, and click beside the coil. Expected: it
   joins as `11/12/14`; a second one joins as `21/22/24`, and a NO contact after that as `33/34`.
2. **Families.** In Find Symbol type `protection`. Expected: the RCBO, the RCDs, the fuse terminal
   and the five fuse banks. `sensors`, `connectors` and `plc` list theirs.
3. **A fuse bank.** Place a **Fuse bank, 8-way** as a new device. Expected: `-F01` with connection
   points `L+`, `M`, `1`…`8`. Typed: `setprop {"component":"<its id>","key":"rating.3","value":"2A"}`
   stores fuse 3's rating; `rating.13` is refused with `-F01 has no fuse 13.` Checks does not list
   its unwired `M` and outputs, only `L+`.
4. **Prefixes are registered defaults.** Typed: `defaults {"group":"Tag prefixes"}` lists every
   symbol's built-in prefix (`symbol.fusebank8.prefix` = `F`, from `builtin`). Set the fuse bank's
   prefix to `FB` in **Project settings ▸ Tags** (or `tagsettings {"prefixes":{"fusebank8":"FB"}}`).
   Expected: `defaults` now says `FB`, from `project`, and the next fuse bank placed is `-FB01`.
