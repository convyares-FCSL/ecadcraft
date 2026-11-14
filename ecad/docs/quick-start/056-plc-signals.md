# 056: the PLC signal list and PLC sync

Brief 056, UI rev 4 (board 2.03). Project ▸ PLC is the project's signal list: every channel has a
PLC tag, a safe state and what it drives, and the list stays in step with the PLC programming
project through a tag table that ECADCraft exports and imports back. Files only: there is no live
link to a PLC tool. The one format is the generic CSV tag table (`name, address, type, comment`).

## By clicking

1. Open a project with a PLC rack and cards (or place a CPU and a DQ card, wire a few outputs and
   fence them with **Place Channels**).
2. **Project ▸ PLC**. The rack heading reads `Rack 1  -A01 to -A03 · 3 of 4 slots used · …`. Each
   tile shows `Slot n` and its kind words (`16 outputs`), the tag, the description or the type
   name, the squares, the address range and `n of m in use`. A free slot says `Slot 4 is free`.
3. Select a card's tile. The card header reads `-A03 · output card · slot 2  16 channels, 9 in
   use, drawn in detail on page 5`, with the chip `PLC: not imported`, **Compare** (greyed until a
   tag table is imported), **Export tags** and **→ Page 5**.
4. Type tags in the **Tag** column (`Valve_Inlet`). A tag is unique in its PLC without case: the
   same tag on another channel is refused with `Tag "Valve_Inlet" is already -A03 channel 2
   (%Q2.2).`
5. **Safe** shows the default (`Off` for digital outputs) until you type `On`, `Off` or `Hold`.
   Analogue outputs take `Hold`, `Min`, `Max` or a value (`4 mA`). Inputs show `—`.
6. **Drives** shows the end device, through terminals and plugs (`-H03:X2`); click it to go there.
7. The card header's **…** ▸ **Make tags** fills the empty tags of used and reserved channels from
   their descriptions (`Inlet valve open` → `Inlet_valve_open`), unique with `_2`, `_3`.
8. **Export tags**: the path row offers `<project> PLC tags.csv`; **All racks** exports every rack.
   The echo counts the channels left out (`left out 1 without a tag`). Export changes nothing.
9. Edit a name in a spreadsheet, save it as CSV (comma, or `;` with a BOM, both read).
10. **Import tags** (tab bar): give the file, **Preview** shows the comparison and a summary line
    (`310 tags read; 10 with an I/O address. 300 memory and data-block tags skipped. 3 differences
    on -A03, 1 not on any card.`). **Keep the snapshot only** or **Also take the PLC's names**, then
    **Import** writes one transaction `Import PLC tags`; `u` undoes it whole. **Cancel** writes
    nothing.
11. The chip now reads `⚠ PLC: 3 differences`; the **PLC** column says `✓ in sync`, `⚠ Valve_In`,
    `ⓘ not in PLC` or `⚠ only in PLC: Spare_Out4`. **Compare** shows both sides with the address
    between; **Use PLC's** takes one name, **Use the PLC's name for all n** takes them all, each in
    one transaction. **Back to the list** or `Esc` returns.
12. The address style **percent** (`%I0.0`, `%IW64`): `plcaddress {"rack":"-A01","style":"percent"}`.

## Commands (also MCP)

`plcchannel {card, channel, tag?, safe?}`, `plcmaketags {card | rack}`, `plcexport {format?, rack?,
path?}` (without `path`: the text), `plcimport {path | text, dry_run?, take?: none|names}`,
`plcsync {card?, rack?, state?}`, `plctake {items | card | all, what?: name|address}`. With writes
off, `plcexport`, `plcsync`, `iolist` and `plcimport {dry_run: true}` work and record nothing; an
agent passes the table as `text`, never a `path`.

## Limits

- Generic CSV only; TIA Portal, TwinCAT and the like come in 0.5.0.
- A tag table keeps at most 20 000 I/O tags, and its snapshot at most 1 MB (the file format's
  longest string); a larger one is refused with words.
- The Test column shows `—`: tests come with the Tests tab (0.2.3).
- Provisional addresses (the board's legend) are a separate brief; the square is *reserved*.

## Fixtures

`ecad/tests/fixtures/plc/P56-tags.csv` and `P56-tags-b.csv` (synthetic, this project's own work).
