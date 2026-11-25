Historical record (2026-11-14). Not current; see [user/plc.md](../../user/plc.md).

# 056: the PLC signal list

Brief 056, UI rev 4 (board 2.03), with the owner's change of 2026-11-14. Project ▸ PLC is the
project's signal list: every channel has a PLC tag, a safe state and what it drives. Tags go out to
a PLC project as a generic CSV tag table (**Export tags**) and come in from one as a one-way fill
(**Import an I/O list**). There is no sync with an outside PLC: nothing of an imported table is
kept, and nothing is compared afterwards.

## By clicking

1. Place a CPU (Place ▸ search `rack`). The book gains the generated **PLC racks** page in the same
   step (`u` takes both away). Place a DQ card, wire a few outputs and fence them with **Place
   Channels**.
2. **Project ▸ PLC**. The rack heading reads `Rack 1  -A01 to -A03 · 3 of 4 slots used · …`. Each
   tile shows `Slot n` and its kind words (`16 outputs`), the tag, the description or the type
   name, the squares, the address range and `n of m in use`. A free slot says `Slot 4 is free`.
3. Select a card's tile. The card header reads `-A03 · output card · slot 2  16 channels, 9 in
   use, drawn in detail on page 5`, with **Export tags**, **→ Page 5** and **…** ▸ **Make tags**.
4. The right panel edits the card, as board 2.03's: **Type** (DI DQ AI AO RTD IOL), **ID**,
   **Description**, **Rack**, **Slot**, **Function**, **Location**; **Addresses** (Start, typed as
   `%Q4.0` or `4`, empty for automatic; Rule); **Detailed page** (Go to page, Draw the missing
   channel); **Parts** (the main part's maker and part number, **+ Accessory** for a front
   connector and the like). Each change is one step; `u` undoes it.
5. Type tags in the **Tag** column (`Valve_Inlet`). A tag is unique in its PLC without case: the
   same tag on another channel is refused with `Tag "Valve_Inlet" is already -A03 channel 2
   (%Q2.2).`
6. **Safe** shows the default (`Off` for digital outputs) until you type `On`, `Off` or `Hold`.
   Analogue outputs take `Hold`, `Min`, `Max` or a value (`4 mA`). Inputs show `—`.
7. **Drives** shows the end device, through terminals and plugs (`-H03:X2`); click it to go there.
8. **…** ▸ **Make tags** fills the empty tags of used and reserved channels from their descriptions
   (`Inlet valve open` → `Inlet_valve_open`), unique with `_2`, `_3`.
9. **Export tags**: the path row offers `<project> PLC tags.csv`; **All racks** exports every rack.
   The echo counts the channels left out (`left out 1 without a tag`). Export changes nothing.
   Values a spreadsheet would run as a formula (they start with `=`, `+`, `-` or `@`, as device
   tags like `-A03` do) are written with a leading `'`, so they open as text; importing takes the
   `'` off again.
10. Edit names or comments in a spreadsheet; save it as CSV (comma, or `;` with a BOM, both read).
11. **Import an I/O list** (tab bar): give the file and **Import**. Each row's channel (by address)
    takes the row's name as its tag and its comment as its description, in one step; rows at an
    address no card has are named in the echo. If the result would give two channels of one PLC the
    same tag, nothing is imported and the path row says which two. A file that is not a tag table,
    or has no `address` column, is refused in words there.
12. The address style **percent** (`%I0.0`, `%IW64`): `plcaddress {"rack":"-A01","style":"percent"}`.

## Commands (also MCP)

`plcchannel {card, channel, tag?, safe?}`, `plcmaketags {card | rack}`, `plcexport {format?, rack?,
path?}` (without `path`: the text), `plcimport {path | text, name?, dry_run?, what?:
both|tags|descriptions}`, `plccardtype {card, type}`. With writes off, `plcexport`, `iolist` and
`plcimport {dry_run: true}` work and record nothing; an agent passes the table as `text`, never a
`path`.

## Limits

- Generic CSV only; TIA Portal, TwinCAT and the like come in 0.5.0.
- A tag table is read up to 16 MB and 20 000 I/O tags; more is refused with words.
- A card's type changes among the I/O types only (their bodies' points are the same).
- The Test column shows `—`: tests come with the Tests tab (0.2.3).
- Provisional addresses (the board's legend) are a separate brief; the square is *reserved*.
- The **PLC racks** page stays when the last rack is deleted (it then says `No PLC racks.`).

## Fixtures

`ecad/tests/fixtures/plc/P56-tags.csv` and `P56-tags-b.csv` (synthetic, this project's own work).
