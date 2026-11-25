Historical record (2026-11-17). Not current; see [design/views.md](../../design/views.md).

# 064: one update pipeline for derived views (R9)

Nothing to click: this lane changes how the program keeps its derived views (references, checks,
reports, the Devices and Properties feeds, the page tree) up to date after an edit, not what a
person sees. Every op is classified once (`ecad_model::effect::Effects::of`), and the three commit
paths (a command, a drawing edit, undo and redo) all run the same `Views::begin` / `finish` and
`Project::apply_changes`. Try any edit, undo and redo as before: the drawing, the lists and the
panels should look exactly as they did.

Progress (automated checks only, not owner-tried):

- Step 1: `project.rs` split into `project/` (a pure move).
- Step 2: `Effect` and `DerivedView` in ecad-model; an empty `Views` in ecad-electrical, wired into
  all three paths; `Document: Sync` holds (asserted at compile time), so the `parallel` feature of
  step 12 is possible. The views themselves arrive one per step as the type lanes they read land
  (design 072 §12.3).
- Step 3, first feed: the Pages tree feed (`TreeFeed`) decides when the Pages panel reads its list
  again. To try it: rename a page, move one into a folder, and undo; the Pages panel follows each
  change. Changing only a page's display settings leaves it alone.
- Step 3, second feed: the Devices feed (`DevicesFeed`) decides which rows of the Devices list
  are read again. To try it: open the Devices tab, change a device's description, renumber the
  page it is on, and move a page before it in the Pages list; its row (description, then its
  `/page.column`) follows each change, and undo puts it back. Renaming another page or editing
  a wire leaves the list alone. The Properties feed follows once lane 072 B has moved each
  group's data into fields.
- Step 4, the frame (`FrameView`): what the active page's frame showed is now one of the views,
  and every commit path (a drawing edit too, which it skipped before) decides from it whether
  the title block is drawn again. To try it: with a page open, change the project's title in
  Project properties, renumber the next page, and add a revision; the title block follows each
  change, and undo puts it back. Drawing a wire leaves the title block alone.
- Step 5, first index: the connector index (`ConnectorsView`) is now one of the views, built on
  open and branch switch and kept from each commit. To try it: place two connectors (Connector,
  both halves) on different pages, give one its own socket side name in Properties, then set the
  project's socket side name in Settings; only the other connector's half tags change, on its
  page, and undo puts them back.
- Step 5, second index: the reference index (`RefsView`) is now one of the views too, with the
  cable references, contact texts and mirror tables, and continuation arrows it redraws, and the
  line about a reference left without its other end. To try it: place a coil on page 1 and one
  of its contacts on page 2, then renumber page 1; the contact's reference text and the coil's
  mirror table follow, and undo puts them back. Deleting one end of a two-ended cable reference
  prints the orphaned-reference line, as before.
- Step 6, the drawn pages (`PagesView`): which pages a commit could change, and which page an
  undo or redo brings to the front, now come from one view on every path; a drawing edit's
  change log now also names the other pages it changes (design 064 D2). To try it: move a
  symbol, open another page, and undo: the page of the move comes to the front with the symbol
  back in place.
- Step 9, generated pages (`GeneratedView`): the cover, contents and revisions pages decide from
  one view whether they are drawn again. To try it: with a cover, a contents page and a
  revisions page in the book and a schematic page in front, place the project's first coil (the
  cover's symbol legend gains it, a second coil leaves the cover alone), rename the page (the
  contents page lists the new title), and add a revision (the revisions page shows the row).
  The report pages (parts list, terminals, cables, I/O list, racks) follow once `OutputsView`
  lands.
