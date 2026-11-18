# 072 Typed object model: lane D (wires, routes, cable cores)

What changed for someone using the app:

- **A drawn line is a route; a wire runs between two connection points.** Drawing a line from
  one connection point to another makes one wire. A line that ends on the middle of another line
  (a T) joins that net: the new wire runs from the new point to the nearer end of the wire the
  other line carries (the far end when the nearer one is full). Wires are numbered in drawing
  order.
- **The Junction tool is gone.** A T is made by drawing onto a line. **No dot is drawn at a T
  yet**, on screen or in print: lane F's step F3 draws junction dots and T indicators from the
  lines (they are never stored). Until then a T looks like a line ending on another line.
- **A loose line has no wire yet.** A line with a free end is drawn, can be slashed by a cable,
  continued and given terminals, but carries no wire. The Checks list shows **Line not connected
  at both ends: no wire yet** for it, with its page and position. When both ends land on
  connection points the wire forms, and a cable core on the line moves onto that wire.
- **Cable cores remember where they are**: on a wire, on a loose line (with the end that has
  landed), or not placed.
- **Wire numbers: one per net, or one per wire** (Project Settings, Wires, **Wire numbers**).
  One per net is how it was: every wire of a net shows the net's number. One per wire gives
  each wire its own number, in the net's wire order; the first wire keeps the net's number. The
  switch renumbers every wire in the same edit, and one undo takes both back. Fixed numbers stay.
- A wire across two pages through a cable reference does not form yet (lane F, step F4 brings
  it back through named points on both pages).

## Try it (click only)

1. Open or make a project with two devices on one page.
2. Pick **Wire** and draw from a connection point of the first device into empty space; finish
   the line there. Open **Checks**: the row **Line not connected at both ends: no wire yet**
   names the page and the place.
3. Pick **Cable Slash** and draw a short slash across that line. The cable is made with one core.
4. Pick **Wire** again and draw from the free end of the line to a connection point of the
   second device. The check row goes; the wire appears in the wire list, and the cable's core is
   that wire (cable view or hover the slash).
5. Draw a third device and a line from one of its points onto the middle of the first line: the
   wire list shows a second wire, from the new point to the nearer end.

6. Open **Project Settings**. Under Wires, **Wire numbers**, pick **One per wire** and press
   OK. The two wires of the net from step 5 now show different numbers on the page and in the
   wire list; the first keeps the old one. Undo: both show the net's number again.

Not yet tried by the owner. Checked by the headless tests named in the lane's tests
(`lanes::lane_072d`, ecad-model, ecad-file, ecad-inspector, ecad-cadcraft, ecad-mcp, ecad-app).
