# 040: the canvas (U4a), how to try it

Brief [`040-ui-canvas.md`](../briefs/040-ui-canvas.md); choices and tokens in
[`design/040.md`](../design/040.md). Start the trial as usual (`cargo run -p ecad-trial --release
-- <project.ecad>`).

## What changed

- **The page is paper.** The sheet is `#F6F4EF` with a dark surround and a shadow. A 0.25 mm dot
  grid every 5 mm shows inside the frame once you zoom in far enough (dots at least 10 px apart).
  CADCraft's ViewCube, UCS icon, viewport label and line grid are gone. The developer switch
  (`--cad-ui`, `ui.cadchrome on`) brings the ViewCube and UCS icon back.
- **Dark sheet.** Open the Text panel (Ctrl+/), Commands tab, and run `ui.sheet dark`
  (`ui.sheet paper` goes back, `ui.sheet` alone toggles). The choice is remembered with the shell
  state, per person, not in the project.
- **Markers by meaning**, at a fixed screen size:
  - a connection point is a teal ring;
  - an attached end is a green dot;
  - a loose end is a grey square;
  - an end that looks connected is an amber triangle with "!";
  - an orphan cable reference is a red disc with "!";
  - a provisional tag gets a dotted magenta underline.
- **Wire colours.** Give a wire a colour (Properties, or
  `setprop {"conductor": "<id>", "key": "colour", "value": "GNYE"}`). It is drawn in that colour.
  A two-colour code (GNYE, RDBU, YEBN, GN/YE, GRN/YEL) draws the first colour solid with 2 mm
  dashes of the second. An unknown code draws in ink and adds an information row to Checks:
  "Colour code not defined". A page draws at most 20 000 dashes; past that, wires keep their
  first colour and Checks warns "Two-colour wires drawn in one colour".
- **Printing.** `print.colour` is `mono` by default: black, with the colour code in the wire
  label. No command writes the project value yet (like the other `print.*` defaults; 042's
  Standards will), so for now choose colour per run: `printbook {"colour": "colour"}`, or the
  Print Book dialog's colour choice. Wires then print in their colours, two-colour wires with
  their dashes, and everything else in ink. Markers, the grid and selection never print.
- **Sheet font.** Page text is Barlow Semi Condensed, in the window and in headless prints
  (`ecad-mcp`, Print Book, release) alike. Tags use its SemiBold face. A project that
  sets its own `text.font` keeps it. Heights are **cap heights**: a 2.5 mm tag measures 2.5 mm
  from the baseline to the top of a capital. The values are unchanged; the labels say "cap
  height".
- **Related placements.** Select a symbol of a device. Its other placements on the page get a
  dashed blue outline.
- **First open.** A page you open for the first time opens at the zoom where a tag is 13 px tall.
  After that, the page keeps the zoom you leave it at.

## Check by hand

1. Open your project and look at page 2 against its print. It should look like the print.
2. Print one page mono and one in colour. Compare the GNYE (PE) wires.
3. Run `ui.sheet dark`, then restart the trial. The dark sheet should still be there.

## Not yet

- On screen, ink is CADCraft's black (`#000000`), not `#16191D`. Matching it needs an upstream
  seam.
- The grid dots are painted over the drawing, so a dot can sit on a wire.
- The leader line starts at the companion's edge, not at the row.
- The provisional tag's text keeps its existing magenta. Only the underline is new.
