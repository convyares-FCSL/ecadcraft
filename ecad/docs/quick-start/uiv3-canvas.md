# UI rev 3, section D: the canvas, sheet colour and themes

What section D of UI rev 3 changed, and how to try it by clicking. Boards 1.15 to 1.18 of the
handoff; the build spec is `docs/discussions/ui-2026-11/rev3-build-spec.md` ("Sheet colour and
full dark"). Folded into `docs/quick-start.md` at the phase-end tidy-up.

## What changed

- **Zones: letters down the sides, numbers across the top.** A new project's sheet numbers its
  columns 1–8 across the top and bottom and letters its rows A–F down both sides (owner, UI round
  2: "zones A–F × 1–8"). Before, it was the other way round.
- **References read `2/B5`**: page, then the zone, letter first (contact tables, cross-references,
  cable and continuation references, check rows). This is the new default reference style,
  `page-zone`. Project properties still offer `105-B-3` (the letter first too) and `/105.3` (the
  column).
- **The page-column wire numbering scheme** separates a numbered column from the page: `1.12.`,
  so page 1 column 12 and page 11 column 2 never share a sequence. A grid lettered across the top
  keeps `1A.`.
- **Projects saved before.** A project that kept the default A3 grid stored no grid and no
  reference style, so it opens with the new grid and `2/B5`. A project with another sheet (A2,
  custom) stored its labels and keeps its old grid, with the letter still first (`105/C4`).
  Nothing is migrated.
- **Light and dark interface.** The sun and moon switch at the right of the top bar now works:
  the sun gives the light shell of board 1.17, the moon Graphite. The choice is kept with the
  window layout, not in the project.
- **Sheet colour: Paper, Follow, Dark.** The sheet colour pill in the bottom bar (`IEC · A3 ·
  Paper`) opens "Sheet colour on screen":
  - **Paper**: always light.
  - **Follow**: the dark sheet in the dark interface, paper in the light one (board 1.18).
  - **Dark**: always dark.

  It changes the screen only: PDFs and prints always use paper colours. `ui.sheet follow` does the
  same from the Text panel.
- **The parts list page (BOM by location)** reads as board 1.15: Tags, Description, then Qty
  (right aligned), Maker and Part number; each location heading is ruled off with the frame's
  weight.
- **Compare revisions (board 1.16), layout only.** A header over the canvas (`Rev C → Now, D*`,
  Close compare) and the list of changes under it, with the board's columns. The list says that
  the changes come with Compare; nothing is compared yet.

## Try it

1. Start the trial and open any project (or the demo). Look at a page's frame: 1 to 8 across the
   top, A to F down the sides.
2. Click the **sun** in the top bar: the whole shell turns light. Click the **moon** to go back.
3. Click the sheet colour pill in the bottom bar and choose **Follow**: the sheet turns dark.
   Switch the top bar to the sun: the sheet turns paper. Choose **Dark**: dark in both.
4. Print or export a PDF with the dark sheet on screen: the PDF is on paper.
5. Open a BOM by location page (add one as before if the project has none) and look at its
   columns.

## Limitations

- Settings ▸ Project info's default sheet colour for new projects is section F's (Settings). The
  menu's choice is the person's, kept with the window layout.
- Compare revisions shows its layout only; History's "Compare with C" button belongs to section
  A and calls `Trial::open_compare`.
- Older dialogs (Reports, Release, Sheets, the merge and renumber dialogs) still draw some colours
  of their own and keep them in the light interface.
