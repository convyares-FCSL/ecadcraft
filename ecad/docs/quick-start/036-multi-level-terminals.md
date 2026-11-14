# Brief 036: multi level terminals

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## 5-036. Multi-level terminals (by clicking)

Brief 036, design `ecad/docs/design/036.md`. A page with three or more parallel wires (the
quick-start sample, or draw them); open the **ECAD** tab first. Agent-observed only: not yet tried
by the owner.

1. **L/N/PE.** Click **Place Terminals**, press Enter for a new strip, type `LNPE` at **Type**, `L`
   at **Level**, Enter at **Position** (the next free one), then draw a line across the L wires.
   Repeat with Enter for the same strip, `LNPE`, `N`, and a line across the N wires; then `PE`.
   Expected: one strip `-X01` whose labels read `1.L`, `1.N`, `1.PE` … `3.PE`; each line is one
   step of undo. The echo says `Placed -X01:1.L…3.L on 3 wires (positions 1–3, level L).`
2. **Labels.** Hover a terminal: `-X01:2.N  L/N/PE level N`. Typed:
   `termlabel {"terminal":"<its id>","text":"2N"}` makes that level read `2N`;
   `termlabel {"terminal":"<its id>","position":"2.1"}` renames the whole position (its levels,
   accessories and links follow). `projectprops {"props":{"terminal.level.separator":""}}` makes
   every multi-level label `1L`.
3. **Sensor levels and links.** Place Terminals `SEN` with levels `S`, `+` and `-` on three rows
   of wires. Click **Jumper**, then the `+` level of position 1 and of position 4. Expected:
   `link.1 = 1.2-4.2`; the `+` levels are one net, the `S` levels stay four nets. A pick on another
   level of another position is refused in words.
4. **Fuse and disconnect positions.** Place Terminals `FUSE` across a 24 V wire. Expected: the
   wire breaks into two nets with two wire numbers. Typed: `setprop
   {"component":"<strip>","key":"rating.<position>","value":"2A"}`.
5. **A fuse terminal device.** Place a **Fuse terminal** (034's `fuseterm`) near a strip and wire
   it. Checks shows the Info row `1 fuse terminal is a device of its own` with **Make strip
   position**; click it. Expected: the device becomes a fuse position of the nearest strip on the
   page, its wires and rating kept; `u` restores the device. Typed: `toposition {"device":"<id>"}`.
6. **Plan and BOM.** **Reports ▸ Terminals**: one row per level, labels `1.L`…; free levels say
   `free`. `setprop {"component":"<strip>","key":"terminals","value":"12+2PE+4x3"}`: spare
   positions by type, each three-level spare with three rows. **Reports ▸ BOM**: one row per strip
   per type, Qty in positions (`L/N/PE terminal ×3`).
