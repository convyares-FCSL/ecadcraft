# UI rev 3, section F: Library, Publish and Settings

What section F changed, and how to try it by clicking. Boards 3.01–3.04, 4.01, 5.01 and 5.02 of
`docs/discussions/ui-2026-11/rev3-build-spec.md`. A UI-only round: nothing new is written to a
project. Listed in the index of `docs/quick-start.md` (phase-end tidy-up after M1).

## Library (boards 3.01–3.04)

- **Library ▸ Symbols.** Tabs across the top: Symbols (with its count), Snippets, Parts, Makers
  and Title blocks. Only Symbols has data; the other tabs say what they will hold.
- Beside the tabs: the search ("Find a symbol by name, tag letter or pin"), **Open an .ecadlib**
  (greyed: shared libraries come later) and **+ New symbol**.
- The filter row: Poles, Contact, Standard and Maker are greyed with their reason (the symbols do
  not carry that data yet). **Source** filters: All, Built-in, Project.
- The category tree at the left, with the sources and their counts at its foot. The tiles in the
  centre, grouped under their category. Hover a tile for its connection points and text slots.
- The selected symbol on the right: its picture, its source, About this symbol, how often it is
  used, then **Place on page N** and **Edit a copy** (or Edit symbol).
- There is no inspector rail in the Library or in Publish.
- **New symbol, Make symbol, Edit symbol** (048's builder): the symbol is drawn on a sheet of
  paper on the dark canvas. A card at the top left says what is being made and where from (with
  Back to the page when it came from a page). The grid's words are at the top right, the intent
  line is a pill at the foot. The strip has labelled buttons (Select, Draw, Text, Point, Slot,
  Origin) and Snap, Grid and Ortho at its foot; the builder always snaps to its grid, so those
  show as on. The right panel has a title and titled sections.
- **Before saving** (Edit symbol with placed copies): the choice and **Save and update N** sit on
  the review's title line; the drawer is taller in the Library.

## Publish (board 4.01)

- **Left: output sets.** Full booklet is today's print, with how many pages it includes. Panel
  builder pack, Customer approval and Site wiring are placeholders. At the foot, **Release Rev X…**
  opens 027's Release dialog (it no longer opens by itself when you enter Publish).
- **Centre: every page**, drawn from the model, grouped as in the book, with its place in the
  output. **Untick a page to leave it out**; Select all ticks every page again. Click a page to
  preview it on the right. Pages print in book order: reordering is not offered yet.
- **Right: PDF** (Printer and DXF are greyed). Colour or Black, Line weights, the file. Revision
  clouds, watermark, bookmarks and numbering are greyed. "As it will print" shows the chosen page.
  **Export PDF…** runs Print Book with the ticked pages.

## Settings (boards 5.01, 5.02)

- Opened from the project name menu, between the same top and bottom bars. The project pill is
  outlined and no workspace is lit; the bottom bar shows only the status. Choosing a workspace in
  the top bar leaves Settings, as Esc and **Back to Draw, page N** do.
- The list: Project info, Standards, Pages and sheets, Libraries, Agent access, Versions and
  backups; then Appearance and Shortcuts.
- **Project info**: the fields that exist (title, drawing number, customer, status, the default
  function and location, other properties) in cards, with a title block preview. People and
  approval, site, customer reference, the customer logo, revision letters, language and units are
  dashed placeholders. Save and Cancel at the foot work as the old OK and Cancel.
- **Standards**: IEC, US and Another standard as cards (IEC in use; US comes later), the standard
  header's actions under them, the catalogues as a list on the left, the table and the detail.
- **Pages and sheets**: the old Project settings, with Text styles on its second tab.
- **Libraries**, **Versions and backups** and **Shortcuts** show what exists today (sources and
  counts, Revisions…, the keys) and say what comes.
- **Appearance**: the interface and sheet colour as they are, and the developer switch.

## Checks

- `cargo test -p ecad-trial uiv3_library`: Settings between the bars and its list; the Library's
  tabs, Source filter and no rail; Publish's ticks choosing Print Book's pages.
- Screenshots: `ECAD_TESTER_SHOTS=<folder> cargo test -p ecad-trial uiv3_library` writes
  `uiv3-F-<board>-ours.png` for each board.
