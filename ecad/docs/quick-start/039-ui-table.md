# 039: the one table (U3)

What brief 039 changed, and how to try it by clicking. Folded into `docs/quick-start.md` at the
phase-end tidy-up.

## What changed

- **One table** (`ecad-ui`'s `table`) now draws Checks, Devices and the terminal manager. It sorts
  (click a header: ascending, descending, back to the list's own order; tags in natural order,
  `-K2` before `-K10`), filters (the search field, and chips such as **Provisional** and **Not on a
  page yet**), groups (**Group:** in the toolbar; groups fold), selects several rows (Shift-click a
  range, Ctrl-click to add, Ctrl+A, ↑ ↓), and copies the shown rows as CSV (**Export CSV**; in the
  Project panel it goes to the clipboard until Lists has its own place in U2).
- **Set versus read.** A value you set in the table looks like a field; a value read from the
  drawing is grey text, and when it has a target, clicking it goes there.
- **Inline edit.** Select a row, then Enter or F2 (or double-click a field): Enter writes it as one
  transaction (the same command as on the page), Esc puts it back, Tab writes it and moves to the
  next field. A refused edit shows its reason under the cell, and the cell keeps the old value.
- **Live.** A change made anywhere tints the changed cells for 1.2 s. Only the rows a change
  touched are re-read; there is no refresh button.

Until the new shell (U2) gives Lists its place, the tables sit where the old lists were:

| Table | Where now |
| --- | --- |
| Checks | Project panel ▸ Checks |
| Devices | Project panel ▸ Devices (the selected device's properties stay under the table) |
| Terminals | Reports ▸ Terminals |

## Try it

1. Open a project with devices, checks and a terminal strip, for example
   `cargo run --release -p ecad-trial -- C:\dev\ecad-projects\<project>.ecad`, or the demo
   project built in `docs/quick-start.md`.
2. **Devices.** Project panel ▸ Devices. Click a row: its first symbol's page is shown and all
   the device's symbols on that page are selected (follow-up to 039). Select a symbol on the page: its row is selected and scrolled to. With a row
   selected, press Enter, type a new tag, Enter: the page shows the new tag, the Tag cell tints,
   and History has one entry. Type a tag another device has: the reason shows under the cell.
3. **Checks.** Project panel ▸ Checks. Click a row to go to it; the fix buttons sit at the row's
   right. Sort by Severity or Page; type in the search field to filter.
4. **Terminals.** Reports ▸ Terminals. Inside conductors are on the left (right-aligned), the
   strip in the middle, outside on the right. Position, Label and Rating (on a fuse position) are
   fields: select a row, Enter on Label, type, Enter: `termlabel` runs as one transaction and the
   plan page follows. Links are drawn as lanes beside the terminals.

## Not yet

- Lists ▸ … as main-view tabs (U2 places the views through `lists::show`).
- Export to a file from Checks and Devices (clipboard only), XLSX (M3), proposal rows (M2).
- Cables, Wires and PLC column sets (later managers).
