# 046: graphical terminal-strip pages

What brief 046 changed, and how to try it by clicking. Folded into `docs/quick-start.md` at the
phase-end tidy-up.

## What changed

- **A terminal plan page has a style**: `table` (028's plan, unchanged) or `graphic`, a picture of
  each strip as it is wired on the rail. It is the default `terminal.plan.style`, set per page or
  for the project; the built-in is `table`, so existing books do not change.
- **Pages ▸ New page ▸ Terminal strips** (after **Terminal plan**) makes a graphic page and opens
  it, titled `Terminal strips`. It shows every strip, each from a new sheet; `newpage
  {"generated": "terminals", "style": "graphic", "strip": "<id>"}` makes a page of one strip.
- **On the page**, per strip: a tag box (tag, location, counts); one column per level, positions
  touching, the levels of an L/N/PE position staggered in one box; inside conductors go up to their
  targets, outside conductors down, each with its wire number and, for a cable core, its core
  label; the terminal's own place (`1-B-3`) beside its label. PE levels are hatched, spare
  positions dashed, a fuse shows its break mark and rating, free levels a dash. Links run in lanes
  in the band's upper half with a dot on each linked level, wire bridges in the lower half; end
  stops, end plates, partitions and strip labels are columns of their own; markers are small boxes
  on top. Under the outside targets each cable has a bar under its cores and a box with its tag,
  type and maker, and an arrow to its far devices (tag and description, at most three).
- **Long strips** run onto further sheets; a position is never split; a link or cable that runs on
  draws its continuation (a link stub, a cable box saying `(cont.)`). Each sheet is titled
  `Terminal strips: -X01 <description> (2/3)`; page numbers and the book's Total count every sheet.
- **Text sizes** follow the text styles (Table, Reference, Wire label, Cable text, Description);
  changing them changes both styles. Everything prints in ink; a missing value is the usual `?`
  box.
- **Switching**: `setdefault {"key": "terminal.plan.style", "level": "page", "page": "<id>",
  "value": "table"}` (or Settings ▸ Standards, group **Terminal strip pages**) switches one page;
  at project level it switches every page with no style of its own. One transaction, undoable.
- The layout lengths are defaults too (Standards, **Terminal strip pages**): column width, body
  height, lead length, band position, tag box width, cable bar gap, right margin, tag size.

## Try it

1. Run the trial with a project that has a terminal strip, for example the demo project built in
   `docs/quick-start.md` with Place Terminals across a few wires and a cable slashed across them
   (028's quick start).
2. **Owner check.** In Pages, open the **v** menu beside **+** and click **Terminal strips**. The
   new page opens: look at one strip against your drawing pack.
3. Go back to the schematic, place a terminal on another wire of the strip (or change a wire
   number), and come back: the strip page shows it.
4. Select a cable and change its **type** in Properties: the cable box on the strip page shows the
   new type.
5. **Publish ▸ Print Book** with the strip page in the book: the PDF's sheets match the page.

## Checks run (worker, not owner-tried)

- `cargo test -p ecad-cadcraft lane_046`, `cargo test -p ecad-mcp lane_046`, `cargo test -p
  ecad-trial lane_046` (screenshots with `ECAD_TESTER_SHOTS`).
- Nobody has clicked through it on a display yet.
