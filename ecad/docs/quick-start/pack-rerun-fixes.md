# Pack re-run fixes (2026-11-14)

Eleven bugs found by the drawing-pack recreation re-run, and what each turned out to be. Listed
in the index of `docs/quick-start.md` (phase-end tidy-up after M1). The tests are `pack_rerun_fixes` in
`ecad-cadcraft` (one or more per bug) and in `ecad-mcp` (the changed commands through MCP).

## What changed

| # | Bug | Cause and fix |
|---|---|---|
| 1 | A project symbol's point with an empty label printed its 32-digit ID after a merge. | Merging named an unlabelled point by its ID. It is named by its key now and stays unlabelled, so no label prints. Joining a part keeps an empty label empty too. |
| 2 | `place {symbol: <project symbol>, tag}` ignored `tag`. | The project-symbol path never read it. A typed tag now names the new device, as for built-in symbols; a tag a device already holds joins that device; `tag` with `device` is refused. |
| 3 | With the tag dash off, a device in another location printed `+X1K01`. | The location was glued to the bare tag. It now prints `+X1-K01`: IEC 81346 separates the location and product aspects by their prefix signs. |
| 4 | Contact tables of coils drawn close together overlapped. | Each table was placed on its own. Tables on one page now clear each other: left to right, a table that meets an earlier one moves right past it, or under it when that would leave the free area. A table placed by hand never moves. |
| 5 | A card drawn in several calls split into several frames. | A cell added with `place` sat with its insertion point at `at`, 5 mm off the fence-drawn cells' line. A channel placed onto a card now goes onto the line of that card's cells on the page when it is within 10 mm. |
| 6 | `plcchannels` refused AI and RTD cells across their own wires. | Each wire got a cell. When the crossed wires come in groups spaced as one cell's points (AI 10 mm, RTD 10 mm × 3), each group is one cell with each point on its own wire. |
| 7 | After raising a card's channel count, the new channels had no address. | The repro wrote `setprop {key: "channels"}`, a free property; the count is `plc.channels`, so the card stayed at 4 channels and 4–7 fell past its address block. On a PLC card, `channels` now means `plc.channels` (as `terminals` is a strip's offer). |
| 8 | An output card on wires running down was not turned. | `plcchannels` always placed at 0°. A cell whose wire runs further down than up from the fence is now turned 180°, its frame and texts above its points. |
| 9 | A typed `PE1` became `PE01`. | **By design.** A tag in the usual letters-and-number form is stored with the project's tag padding (default 2 digits), and `PE1` and `PE01` are the same tag. `tagsettings {padding: 1}` keeps `PE1`. |
| 10 | `plcchannels` landed an IO-Link port's L+ on the wire. | The first point always landed. An IO-Link port now lands C/Q by default, and the new `point` parameter (`point: "L+"`) chooses for any type. |
| 11 | 4-pole power contacts showed no coil reference. | Only the 3-pole set was known to carry poles. The 4-pole set now shows its coil and has rows in its coil's table. |

## Try it

1. **Bug 4.** Place four coils 15 mm apart near the bottom of a page, and an NO contact for each
   on another page. Back on the first page, the four tables sit side by side without overlapping.
2. **Bug 5.** Draw four vertical wires 20 mm apart. Place Channels across the first two, then
   place a DI channel of the same card on the third wire, a little off the others' line. The card
   has one frame and one name.
3. **Bug 6.** Draw two vertical wires 10 mm apart and Place Channels with type AI across both.
   One AI cell lands, its + on the left wire and its − on the right.
4. **Bug 8.** Draw two wires from near the top of a page downwards, and Place Channels with type
   DQ across them just under their top. The card's frame and texts are above the points.
5. **Bug 11.** Place a coil on one page, and 4-pole power contacts joined to it on another. The
   power contacts show the coil's place.
