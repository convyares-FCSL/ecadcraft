# UI rev 3, section C: tools, placing and the wheel

Boards 1.09 to 1.14 of the rev 3 handoff, plus the Snap, Grid and Ortho popouts. This round
changes the layout only: every item runs a command that already existed, and an item the board
names with no command yet is greyed in place.

## Try it

Start the trial with a project open, in Draw.

1. **Flyouts.** Hover any group on the left strip (Draw to Location). The flyout shows the
   group's items; the last-used tool is lit.
   - Terminal: Terminal, Terminals across wires, Draw a link (bridge), Swap top and bottom
     (greyed), Reorder the strip (greyed), Renumber…, and **Terminal manager** at the foot
     (Project ▸ Terminals).
   - PLC: Rack overview (Project ▸ PLC), Detail page…, Card…, Channel… (the card commands'
     templates, as Ctrl+K ran them).
   - Wire: Wire, Bridge, Continuation (places the continuation arrow).
   - Location: Location box; Function box, Function and location box, Device or PLC outline
     greyed.
   Hover a greyed item: its tooltip says it is not available yet.
2. **Popouts.** Hover Snap, Grid or Ortho at the foot of the strip. Snap: on/off and spacing.
   Grid: shown, spacing, major lines. Ortho: 90° only, 45° steps, or free. A click on the button
   still flips it.
3. **Placing.** Device ▸ Symbol. The picker opens at the canvas's top left: Symbol | Connector,
   a search, tiles. Click a tile: the picker folds to a chip ("as -K02"), and the intent line at
   the top of the canvas says what the click does. Press **P** (or Change on the chip) to open
   the picker again.
4. **Connector maker.** Device ▸ Connector (or Connector in the picker). The sections fold; the
   one the tool is asking opens, and its chips answer it. Variants the library does not have yet
   are greyed.
5. **Wheel.** Right-click a device. The wheel is about 190 px across with icons only; the slot
   under the cursor names itself in a tooltip. A greyed slot says why.

## Keys

Kept: **V** Select, **D** Draw, **T** Text, **W** Wire, **C** Cable (groups that still exist),
**P** reopens the picker while placing. No new keys.

## Automated checks

`cargo test -p ecad-trial uiv3_tools`; `cargo test -p ecad-cadcraft actions_contracts`.
Screenshots: set `ECAD_TESTER_SHOTS` to a folder before `uiv3_tools`.
