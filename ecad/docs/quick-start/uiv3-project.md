# UI rev 3, section E: the Project workspace (boards 2.01–2.06)

UI only: the Project workspace (was Lists) laid out to the rev 3 boards. No new commands, no
format change. Agent-observed (headless screenshots); not yet tried by the owner.

Open the demo (`cargo xtask demo`, then open `C:\dev\ecad-projects\demo.ecad`) and press
**Project** in the top bar (Ctrl+2).

1. **Tabs.** Devices, Terminals, PLC | Wires, Cables | Reports, each with its count. At the right
   of the head: the filter, Import (greyed: no importer yet), Export (copies the rows shown as CSV
   and says how many) and **Open beside the page** (the page opens beside the table; press again
   to close it). The foot says what the colours mean and the tab's counts.
2. **The right panel.** Click a row: the inspector opens on it, showing that item only: its name,
   where it is drawn (→ goes there), its own checks, then folding sections. Devices keeps the
   property editor (Tag, Description, Maker, Part number…: the same `setprop` as the page).
3. **Devices.** Tag, Description, Function, Location, Main part (or "No parts yet"), Parts, Symbol,
   On page ("not yet" for a device on no page). The chips and Group menu sit beside the filter.
4. **Terminals.** One plan, a group per strip, under three bands: inside (read), the strip (what
   you set: Terminal, Label, Rating), outside (read).
5. **PLC.** Place a card or two (Draw ▸ PLC). The rack line with the legend, then a card per slot
   at the board's scale (type at the right of the head, addresses under the squares, "n of m in
   use" and the page link at the foot), then the selected card's line and its channels.
6. **Wires.** Every wire, grouped by potential (None, Potential, Page, Cable at the right).
   Ferrules and Length show a dash: they come with the cable manager. Status says ok, check or
   provisional.
7. **Cables.** The cable table, the selected cable's line, a placeholder where the cable will be
   drawn end to end (cable manager), then its cores.
8. **Reports.** The five reports there are today; the one selected shows its preview under the
   list (today's Reports window). Layout, In the book, generation and the generator's settings are
   greyed or a dash: the report generator is not built.

The Check list is no Project tab any more: a card's "1 check" opens the right panel's Checks,
filtered to the card.
