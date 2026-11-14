# UI rev 3, section A: the left panels

Boards 1.02 (Book pinned, two pages ticked), 1.03 (Devices floating), 1.04 (History) and 1.08
(the page well's hover preview). The build spec is
`docs/discussions/ui-2026-11/rev3-build-spec.md`. Folded into `docs/quick-start.md` at the
phase-end tidy-up.

## Pin or let it close

- Each panel's header has a **pin** and a close button.
- **Pinned:** the panel docks beside the tool strip and the drawing moves over for it. Only one
  panel is pinned at a time: pinning another unpins the first.
- **Unpinned:** the panel floats over the drawing with a shadow. The drawing does not move. A press
  on the drawing (starting to draw) closes it.
- Book starts pinned; Devices and History start unpinned. The pin is remembered.

## Book (board 1.02)

- **Find a page** filters the pages by number or title.
- Groups (folders) show a folder icon, the name, the page count and **+** (a new page in that
  group). Click a group to fold it.
- The header switches between **thumbnails** and a **list**.
- The **open page** has a 3 px blue bar, a blue outline on its thumbnail and a blue number.
- **Tick** pages with their checkbox (or Ctrl+click): a ticked page has a ticked checkbox and a
  plain grey ground. The footer then shows "N pages selected", **Clear**, **Move to group**,
  **Duplicate** and **Delete**.
- With nothing ticked, the footer has **New page** (its ▾ lists the page kinds) and **New folder**.
- Drag, Alt+Up/Down, F2, double-click on the number and the right-click wheel work as before.

## Devices (board 1.03)

- Search, then the grouping: **None, Location, Function, Type** (Location by default).
- **Not on a page yet** comes first, with a drag grip on each device.
- Groups fold; a group shows eight devices, then "n more".
- A click goes to the device's first symbol and selects its symbols.
- **Project →** in the footer opens Project ▸ Device list, where many are edited at once.

## History (board 1.04)

- **Rev D\* pending**, after which revision, the checks' verdict, **Release D…** and
  **Compare with C**.
- **The agent**, while one is connected: "Claude is writing on branch main", Pause or Resume, and
  disconnect. The agent writes on the branch shown (Carl's ruling), so there is no Review and
  merge.
- **Branches:** the branch shown ("you are here", ticked), other branches with how far ahead and
  **Merge** (020b's merge dialog), merged and archived ones greyed on request. **New branch**.
  Double-click switches; right-click for rename, archive, merge and update.
- **Changes:** the branch shown's steps, newest first, with the line down the left, each with its
  author's chip, time and branch. An agent's session folds to one row with Undo session while
  live. Select a change for Show, Branch from here and Restore. **Show earlier changes**.
- **Versions:** named versions and released revisions, with Compare. **Name this one**.

## Page well (board 1.08)

Hover a page button in the bottom bar: a preview shows above the bar with the page's number,
title and group, and "click to go".

## Not available yet (placeholders)

- Book: **Duplicate** and **Delete** (no command duplicates or deletes a page).
- Devices: dropping a "Not on a page yet" device on the sheet says placing from the list is not
  available yet.
- History: **Compare** (revision and versions), and the Changes filters other than **All**.
- The thumbnails and the well's preview are drawn sketches of a page, not renders of it.
