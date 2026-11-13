# Brief 051: PLC visuals and the rack overview

Part of the [quick start](../quick-start.md). Design: `docs/design/051.md`; built on
`feature/plc-visuals`. This file covers the first part (independent of the style model, brief 050);
the card look on the page (cells 20 mm wide, one frame per card, the power end, break edges, the
two new text styles) follows when 050 lands and is added here then.

## What this part changed

| Where | What is there now |
| --- | --- |
| Lists | The tab **I/O** is now **PLC**. It opens on the rack: one chip per rack (only when there is more than one rack or cards in no rack), and on the right `n channels, m in use` |
| Lists ▸ PLC, the strip | One tile per slot, the rack first and the next free slot last: slot, tag, type and count (`DQ 16`, or the card's own description), address range, one square per channel (8 a row: grey in use, outline free, pink reserved, red has a check) and a link line: `page 108`, `pages 106, 107`, `not drawn yet` with **Make its page**, `· 1 check`, or **Place a card…** on the free tile. Under the strip, the legend |
| Lists ▸ PLC, the table | **This card** (the selected tile's card, without Slot, Rack, Card and Type) or **All cards** (grouped by card, as the I/O list was). **Description**, **Address** and **Pins** are inputs; they run `plcchannel`. A typed address shows `*`; a refusal shows on the cell |
| Make its page | A new page after the rack's last card page, titled `-A03 DQ 16` (Settings ▸ Standards ▸ PLC: **Title of a card's page**), with every channel not drawn yet in rows and the card body after them when it is drawn nowhere. One step: U removes it. Command `plcpage {card, after?}` |
| The I/O list sheet | One ruled table per card (outer box, every row and column line), its heading above it: `-A01 slot 2 · -A03 · DQ 16 · <part number> · 9 used, 1 reserved, 6 spare`. Slot and Card are no longer columns. Cards follow each other on a sheet; a card that does not fit goes to the next sheet; a card longer than a sheet continues with `(continued)` |
| PLC racks sheet | New page kind (New page ▸ PLC racks, or `newpage {"generated":"plcrack"}`): per rack a heading and its tiles in ink, 9 a row on A3, squares, and each card's place (`→108-B-2`) or `not drawn`. Without PLC cards it says `No PLC racks.` |
| `plcracks` | Each card also gives `pages`, `checks` and `range {inputs, outputs}` |

## Try it

1. Make a CPU and three cards (045's steps 1–3). Draw eight wires 20 mm apart and **Place
   Channels** across them for the DI card.
2. **Lists ▸ PLC.** Expected: tiles Rack, Slot 1, Slot 2, Slot 3 and a dashed Slot 4 `Free`; the
   DI tile shows eight grey squares and `page 1`; the others `not drawn yet` and **Make its page**.
3. Click the DQ tile. Expected: its head reads `Slot 2 · selected`, the table shows its 16 rows.
   Double-click a Description cell, type `Pump 1 run`, Enter. Expected: the square turns pink
   (reserved).
4. Click a square of the DI tile. Expected: the DI tile is selected and that channel's row is
   selected. Double-click it: the page shows its cell.
5. Click **Make its page** on the DQ tile. Expected: a new page `-A03 DQ 16` with sixteen cells;
   the tile now reads `page n`. U removes the page.
6. Add pages of kind **PLC racks** and **I/O list** and look at them.

## Limitations of this part

- **Place a card…** starts Place Symbol and says which slot the card will take; it does not yet
  fill the search with `card` or preset the Rack and Slot prompts.
- The squares' hover names channel and state, not yet the address and what it is wired to.
