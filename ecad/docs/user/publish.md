# Publish: Print Book and PDF

Print Book prints the project's pages, generated pages included, as one PDF: each page on its own
sheet with its frame and title block, in book order. Printing reads the project and records
nothing. To issue a revision with its PDF and lists, use a release ([releases.md](releases.md)).

## The Publish workspace

Press **Publish** in the top bar (Ctrl+4).

- **Left: output sets.** **Full booklet** is the print you have today, with how many pages it
  includes. **Panel builder pack**, **Customer approval** and **Site wiring** are placeholders until
  output sets exist. At the foot, **Release Rev B…** opens the Release dialog for the pending
  revision; released revisions are in Draw ▸ History.
- **Centre: every page**, drawn from the project and grouped as in the book, each with its place in
  the output. **Untick a page to leave it out**; **Select all** ticks every page again. Click a page
  to preview it on the right. Pages print in book order.
- **Right: PDF settings.** **Colour** (colour or black), **Line weights**, and the **File** to write.
  The paper is each sheet's own size. **As it will print** shows the chosen page. **Export PDF…**
  prints the ticked pages.

The PDF is written to the file you type: the app has no file dialogs. The result says how many
sheets were printed (and how many were generated pages), where, and the file's size.

## What prints

- Every sheet of every page in scope, in book order, with its frame and title block. A report page
  that runs onto several sheets prints all of them.
- **Black** prints everything in ink (a wire label still names the colour code); **Colour** prints
  wire colours and the colours of text styles. The default is the project's print setting.
- A missing value in a generated table prints as `?` in a thin box, black in either mode.
- What is drawn only on screen never prints: the marks of connection points, the agent's last-write
  outline, a selected change's outline, and hidden texts shown dimmed.
- Each PDF page holds only what lies on that page, so a book's file size grows with its sheets.

## Paper and large books

Each sheet is printed on the smallest ISO A paper it fits. A sheet that is not exactly that size is
printed fitted, and the result says so. A book of more than 256 sheets is written as volumes:
`name.pdf`, `name-2.pdf`, …, breaking between pages where possible.

## Typed

`printbook {"path":"C:/tmp/demo.pdf"}` prints every page; `"scope":{"pages":"1, 3"}` a page list
(`100, 105-110`: a range runs in book order), `"scope":{"folder":"<id>"}` a folder with its
sub-folders; `"colour":"mono"` (or `"colour"`), `"lineweights":false`. A page number that does not
exist is left out and named in the reply's notes. It records nothing.

An agent cannot give a `path`: without one, `printbook` returns the PDF as data.

## Limits

- No printing to a printer, and no DXF export: both are greyed in Publish.
- Revision clouds, a watermark, bookmarks and cross-reference links, and page numbering options are
  greyed: they come later.
- Pages print in book order only: reordering comes with output sets.
- No A4 or portrait sheets.
