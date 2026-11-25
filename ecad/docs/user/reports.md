# Reports: the BOM, lists and report pages

ECAD works out its reports from the drawing: the bill of materials (BOM), the wire list, the cable
list and the terminal plans. They are never stored, never edited by hand and stay live: there is no
refresh button. A report can be exported as CSV, or added to the book as a report page that prints
with the drawings.

## Open the reports

Press **Project** in the top bar (Ctrl+2) and choose the **Reports** tab. It lists the reports
there are, what each reads, how it is grouped and what it exports:

| Report | Reads | Grouped by | Exports |
| --- | --- | --- | --- |
| Parts list (the BOM) | Devices, part sets | Location or part | CSV |
| Terminal plans | Terminals, cables | Strip, then position | CSV |
| Cable list | Cables, cores | Cable tag | CSV |
| PLC rack overview | PLC cards, channels | Rack, then slot | (open it in Project ▸ PLC) |
| Wire list | Wires | Wire number | CSV |

The selected report's preview opens under the list, with the tabs **BOM**, **Wires**, **Cables**
and **Terminals**. Click a row to go to its device or wire on its page. **Find** filters the rows;
**Only ?** shows only rows with a missing value.

## Missing values

A value that is missing is shown as a yellow `?` and counted; nothing is refused for it. On a
report page and in print the `?` sits in a thin box and prints black. The Checks list counts the
same gaps (for example **Devices without a part number**); see [checks.md](checks.md).

## The BOM

- Devices are listed with their quantity, tags, description, maker and part number.
- **Group** chooses **By location** (one block per location, the default) or **By part** (the
  whole project together).
- **Columns ▾** adds **Article number**, **Rating ▸** (one column per rating field, such as Rated
  current), and, by location, **Lines under devices** (each device's part lines under it, marked
  `↳`). These choices are project defaults, also listed in Settings ▸ Standards.
- A row whose devices disagree (two article numbers, two ratings) shows the most common value and
  is flagged.

### Part lines

A device or terminal strip can carry more parts than its own: an auxiliary contact block, a
memory card, fuse inserts. Each is a **part line** with a part number, maker, description, article
number, quantity, unit, kit and, for a fuse bank or a strip, `for` (the fuses or positions it
serves, such as `1-2`: its quantity is per fuse). The Properties panel's **Parts** section shows
them read only. Renumbering terminal blocks rewrites a part line's `for` as it rewrites jumper bars.

Typed: `partline {"component":"<id>","add":{"part_number":"AB-1NO","maker":"Generic","description":"Aux block 1NO"}}`;
`"set"` or `"remove"` with the line's number edits or removes one.

### Items

Material that belongs to no device (trunking and DIN rail in metres, crimps, cable ties) is an
**item**: one part, a quantity, a unit (pieces or metres) and a location (its own, else the
project's). It is on no page. On the BOM tab, **+ Item** adds one through the command line's
prompts; a row's right-click **Delete item** removes it. Clicking an item row says
`Items are on no page.`

Typed: `bomitem {…}`; `bomitem {"item":"<id>","delete":true}` removes one.

### Article numbers

An article number can be set on a device, a part line or an item.
`article {"part_number":"AB-1NO","article_number":"A-0110"}` sets it on every entry of that part
at once. Terminal strip parts (terminal types, accessories, jumper bars) take article numbers the
same way, read from the level their part number comes from.

### Wire by the metre

A wire type with a part number adds one BOM row in metres per part number, colour and size: the
number of wires times the type's allowance, rounded up to 0.5 m, marked "estimate". See
[styles.md](styles.md).

## Wires and cables

- **Wires** lists each wire with both ends (`-Q02:2`), their locations, size, colour, potential,
  wire type and cable. Size and colour are the resolved ones (the wire's own, else its type's).
- **Cables** lists each cable with its type and ends. **Include not-drawn cables** adds cables
  built in Project ▸ Cables that are not in the drawing yet.
- **Terminals** shows the terminal plan, per strip or all; see
  [terminal-strips.md](terminal-strips.md).

## Export as CSV

**Export CSV…** on a tab writes the rows shown. Type the path (**Save as**), then **Save**. The
result says `Wrote n rows to … (m cells marked ?).` The file is UTF-8 with commas; a cell that a
spreadsheet would read as a formula (a tag such as `-K01`) is written with a leading `'`. The BOM's
CSV has a `Unit` column after `Qty` (`pcs` or `m`), a `Line` column when lines are nested, and
`Article number` and rating columns when they are on.

**Export** in the head of each Project workspace tab copies the rows shown as CSV.

Typed: `bom {}`, `bom {"group":"part"}`, `bom {"columns":["article","rating.current"]}`,
`bom {"path":"C:/tmp/bom.csv"}`, `wirelist {}`, `cablelist {}`. They read only; `reports {}` opens
the window.

## Report pages

A report can be a page of the book, drawn by ECAD and kept up to date: in the Book panel, **New
page ▾** lists the page kinds. Report pages are **BOM by location**, **Terminal plan**, **Cable
schedule**, **PLC racks** and **I/O list**; front pages are **Cover**, **Contents** and **Revision
history** (see [pages.md](pages.md)).

- Nothing can be drawn on a report page: `Page 5 is generated (BOM by location). Draw on another
  page, or add one after it with +.`
- A long report runs onto further sheets of the same page. The page is numbered and counted once;
  its later sheets take the next numbers (`400`, `401`), and a number that another page's sheet
  holds is refused.
- **One sheet at a time.** While a report page is shown, the Book lists its sheets under it
  (`Sheet 2 of 3 · 401`); click one to show it. The page remembers its sheet while the project is
  open. Going to a row from Reports, a list or Checks shows the right sheet.
- Print Book prints every sheet of every page, in order.

Typed: `newpage {"generated":"contents"}`, `openpage {"page":"<id>","sheet":2}` (1 for the first;
the reply adds `sheet` and `sheets`; it records nothing).

## Limits

- The report generator is not built: each report has one built-in table layout; its layout, place
  in the book and settings show a dash or are greyed in Project ▸ Reports.
- No XLSX export.
- No panel for part lines yet: they are the `partline` command; the parts catalogue comes later.
- Import in the Project workspace is greyed: there is no importer yet.
