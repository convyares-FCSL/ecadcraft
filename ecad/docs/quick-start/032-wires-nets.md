# Brief 032: wires nets

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## 5-032. Wires join, cables cross pages, wire labels (by clicking)

Brief 032, design `ecad/docs/design/032.md`. Checked by the automated tests named in the design
(§12); the steps below have not been tried by the owner yet.

1. **Wire** tool: draw a wire from `0,0` to `50,0`, Enter; then start **Wire** again and click
   on the first wire's open end, then `50,-20`, Enter. Expected: the echo says `end A joins wire …
   (now one wire)`; the two parts are one wire with one label; Checks has no Open end row for the
   corner. While the cursor is over an open end the Wire tool shows a green ring and `join`.
2. Type `J` while drawing: the prompt reads `Specify next point (join off) [Undo/Join]:` and ends
   that meet another wire stay open. Expected after the wire: a yellow ring on the open end, a
   **Looks connected** row in Checks with **Join**; **Join** makes them one wire.
3. **Join Wires** (Wiring tools): click where two open ends meet. Expected: one wire; on an open
   end lying on a wire's middle, a junction.
4. A wire drawn from a `0V` wire's open end with potential `24V` is refused: `That wire carries
   0V and this one 24V; joining them would short them. …`; nothing is drawn.
5. Cable across pages: on one page wires from a terminal to continuations `A1…A3`, slashed as
   `-W04`; on another page continuations `A1…A3` and wires on. Slash them there with tag `-W04`.
   Expected: no new cable; Reports ▸ Cables shows `-W04` with 3 cores from the first page's
   terminals to the second's.
6. **Project panel ▸ Project settings ▸ Wires**: **Labels** `One per net per page` (default) or
   `Every wire`, and **Shortest labelled wire** (enabled with Every wire). Expected: with the
   default a net is labelled once per page; Every wire with 10 mm leaves short drops unlabelled.

Typed instead: `wire {"points":[[0,0],[50,0]]}`, `wire {"points":[[50,0],[50,-20]],"join":false}`,
`joinwires {"at":[50,0]}`, `cableslash {"from":[50,-5],"to":[50,25],"tag":"-W04"}`,
`wiresettings {"label_rule":"each","label_min_length":10}`. `defaults {"group":"Wires"}` lists
`wire.join`.
