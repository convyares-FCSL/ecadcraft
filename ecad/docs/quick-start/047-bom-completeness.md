# Brief 047: BOM completeness

Part of the [quick start](../quick-start.md). The BOM (Reports ▸ BOM, the BOM-by-location page
and the CSV) is in [024-outputs.md](024-outputs.md).

## What this brief changed

**Branch `feature/bom-completeness` (brief 047)**, cut from `dev` at `be6f4413`.

| Where | What is there now |
| --- | --- |
| Part lines | A device or strip can carry more parts than its own: an aux contact block, a memory card, fuse inserts. Each is a *part line* (`part.<n>.part_number`, `maker`, `description`, `article_number`, `qty`, `unit`, `kit`, `for`). The command line's `partline` adds, edits and removes them; there is no panel for them until the parts catalogue (2.3). A line with `for` serves some of a fuse bank's fuses or a strip's positions: its quantity is per fuse, and the BOM lists `-F01:1, -F01:2, -F01:4` |
| Items | Material that belongs to no device (trunking and DIN rail in metres, crimps, cable ties) is an *item*: one part, a quantity and a unit (pieces or metres) and a location (its own, else the project's). It is on no page. Reports ▸ BOM ▸ **+ Item** adds one; a row's menu deletes it |
| Article numbers | `article_number` on a device, a part line or an item. `article {part_number, article_number}` sets it on every entry of that part at once. The BOM's **Article number** column is off until you add it (Reports ▸ BOM ▸ **Columns ▾**), then it is in the window and the CSV |
| Reports ▸ BOM | **Columns ▾**: **Article number** and, By location, **Lines under devices** (each device's part lines listed under it, `↳`, with its kit). Both are project defaults (Settings ▸ Standards ▸ BOM). Qty reads `5.6 m` for metres. Clicking an item row says `Items are on no page.` |
| CSV | A `Unit` column after `Qty` (`pcs` or `m`; Qty a plain number), a `Line` column in the nested layout, and `Article number` when the column is on |
| Inspector | A selected device with part lines shows **Parts**: its main part, then each line (quantity, maker, part number, description, article number, kit or `for`), read only |
| Checks | `5 devices and 3 part lines have no part number`, `2 items have no part number`, `6 fuses have no insert` (only once the project lists any insert), `AB-1NO has two article numbers` (**Use … for all**), a `for` naming a fuse the device lacks (**Remove 9**), and a `part.` key ECAD does not read |

Not yet in this branch: the Article column and nested lines on the **BOM-by-location page**
(waits for brief 046's strip pages to reach `dev`), and article numbers on strip parts (terminal
types, accessories, jumpers) with renumbering keeping a strip line's `for` (waits for brief 045).

## Try it

1. Open the demo and **Lists ▸ Reports**, the **BOM** tab.
2. Click **+ Item**. At the command line type `DIN rail 35x7.5`, Enter, `2`, Enter, `Metres`,
   Enter, then Enter three times. Expected: a row `+CP1 · 2 m · · DIN rail 35x7.5` after the
   devices of `+CP1`; one step in History, `Add item`. Click the row: `Items are on no page.`
3. Right-click the row, **Delete item**. Expected: the row goes; `u` brings it back.
4. **Columns ▾ ▸ Article number**. Expected: an empty **Article number** column; Settings ▸
   Standards ▸ BOM says `on` from the project.
5. At the command line: `partline {"component":"<-K01's id>","add":{"part_number":"AB-1NO","maker":"Generic","description":"Aux block 1NO"}}`
   (copy the id from the inspector or `devices`). Then `article {"part_number":"AB-1NO","article_number":"A-0110"}`.
   Expected: a row `1 · -K01 · Aux block 1NO · Generic · AB-1NO · A-0110`; the echo says
   `Set A-0110 on 1 part (1 line).`
6. Select -K01's coil. Expected: the inspector shows **PARTS**: its own part, then the aux block.
7. **Columns ▾ ▸ Lines under devices**. Expected: -K01 has a row of its own, and under it
   `↳ Aux block 1NO`.
8. **Export CSV…**. Expected: the file has `Location,Qty,Unit,Tags,Line,Description,Maker,Part number,Article number`.

## Checks

- *Automated (agent, native Windows 11, 2026-11-13):* `cargo test -p ecad-cadcraft --test all
  lane_047` (11 tests: items in metres and undo; refusals on items; one line on four devices;
  fuse inserts per fuse and the uncovered-fuse check; the article column, command and conflict
  with its fix; kits and the nested layout; missing `for` names and unread keys; the update map;
  hostile values; save and reopen), `cargo test -p ecad-mcp --test all lane_047` (2 tests: every
  write by the agent, refused with writes off, `bom` still reads) and `cargo test -p ecad-trial
  --test all lane_047` (3 tests: the Columns menu, + Item's prompts, the inspector's Parts).
- *Agent-observed:* screenshots in the pull request, from the headless renderer.
- *Owner-tried:* not yet.
