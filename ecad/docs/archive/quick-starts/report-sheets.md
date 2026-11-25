Historical record (2026-11-24). Not current; see [user/pages.md](../../user/pages.md).

# Report pages of several sheets: one sheet shown at a time

What changed for someone using the app:

- **A report page shows one sheet at a time.** A generated page that runs to several sheets (a
  BOM, a terminal strip page in either style, the cable schedule, contents, the revision list,
  the I/O list, a PLC rack page) is still one page, numbered and counted as before. The page
  window now draws only the sheet it shows, at the origin, instead of every sheet side by side,
  so an edit redraws one sheet, not all of them.
- **The Pages tree lists the sheets** under the page while it is shown: `Sheet 2 of 3 · 401`.
  A click on a sheet shows it; the shown sheet is highlighted. The page remembers its sheet
  while the project is open; a sheet past the last (the table shrank) shows the last.
- **Go-to opens the right sheet.** A Go to from Reports, a list, a detail panel or a Checks row
  that lands on a later sheet of a report page shows that sheet, centred on the point.
  Selection, search and Zoom Extents work on the sheet shown.
- **Print Book and PDF are unchanged**: every sheet of every page prints, in order.
- Agents and the command line pick a sheet with `openpage {"page": "<id>", "sheet": 2}` (1 for
  the first). The reply adds `sheet` (the sheet shown) and `sheets` (how many). It records
  nothing and works with writes off.

## Try it

1. Add a BOM page (New Page ▸ Generated ▸ BOM by location) and place enough devices for two
   sheets (about 45). Show the BOM page. Expected: the Pages tree shows `Sheet 1 of 2` and
   `Sheet 2 of 2` under it, and the page window shows only the first sheet, titled `(1/2)`.
2. Click `Sheet 2 of 2`. Expected: the second sheet replaces the first in the same place; its
   Page box reads the continuation number (`401` for page `400`).
3. Place another device on a drawing page and come back. Expected: the BOM page still shows
   sheet 2, with the new row if it lands there.
4. Print Book the whole project. Expected: both BOM sheets print.

## Bench (large project)

Agent-measured with `ecad-bench --part reports|strips|plc` on the large project, release build,
the same machine for both columns:

| Measure | Before | After |
| --- | --- | --- |
| bomcols (a BOM column toggled, the 117-sheet BOM page shown) | 94.65 ms | 15.59 ms |
| spageall (a wire number on a strip, the 252-sheet every-strip graphic page shown) | 436.48 ms | 6.13 ms |
| spageupd (the same, a one-strip graphic page shown) | 5.65 ms | 4.44 ms |
| plcframeupd (not a report page; unchanged by this) | 7.12 ms | 8.14 ms |
