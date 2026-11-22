# 072 Typed object model: lane D (wires, routes, cable cores)

What changed for someone using the app:

- **A drawn line is a route; a wire runs between two connection points.** Drawing a line from
  one connection point to another makes one wire. A line that ends on the middle of another line
  (a T) joins that net: the new wire runs from the new point to the nearer end of the wire the
  other line carries (the far end when the nearer one is full). Wires are numbered in drawing
  order.
- **The Junction tool is gone.** A T is made by drawing onto a line, and a dot is drawn where a
  line ends on another line, on screen and in print. The dot is drawn from the lines (never
  stored): it moves and goes with its line, and cannot be erased or moved on its own. Hovering
  the line says which wire its end joins (`End B joins wire 3 (a T).`), never "Open end". The T
  indicator (which way the wire order runs) is not drawn yet.
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
- **Connection points accept a number of wires** (two unless the symbol says more, for PE bars
  and earth studs). A T near a point that takes more than two joins from that point (a star);
  otherwise the chain goes on from the far end. A point with more wires than it accepts is an
  error in Checks (**Connection point has more wires than it accepts**, e.g. `-X1:3 takes 2
  conductors; 3 are connected`). No built-in symbol takes more than two yet, and the symbol
  builder cannot set it yet.
- **A wire runs from device point to device point across pages.** A cable reference's points
  (step F4) and a continuation's point (step F5) are named points: aids that a wire runs through,
  never to. A continuation joins the points of every continuation with the same name, so a line
  from `-G01:-` to the `0V` continuation on page 1 and a line from the `0V` continuation on page 2
  to `-K01:A2` make one wire `-G01:-` to `-K01:A2`. A line to a continuation with no partner
  carries no wire (Checks still lists **Continuation without partner**). Renaming a continuation
  re-forms the wires of the nets it leaves and joins; a cable core whose wire goes stays on its
  lines. A **Cable Slash** with an existing cable's tag on another page now only shows that cable
  there, over its own wires (no new cores); on a page where none of its wires runs it is refused
  (`-W04 has no wire on this page: …`).

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

6. Draw two more lines from the first device's point to two new points (three wires on one
   point). Checks lists **Connection point has more wires than it accepts** for it; erase one
   line and the row goes.
7. Open **Project Settings**. Under Wires, **Wire numbers**, pick **One per wire** and press
   OK. The two wires of the net from step 5 now show different numbers on the page and in the
   wire list; the first keeps the old one. Undo: both show the net's number again.

8. The junction symbol is gone: a T is a line drawn onto another line (step 5). The symbol
   picker no longer lists **Junction**, and `place {"symbol":"junction", …}` says there is no
   such symbol. A project file from an earlier build that holds a junction opens read-only, and
   the message names the word `junction` in setting `kind`.
   At the T from step 5 a dot is drawn where the new line meets the first one. Hover the new
   line: its end says it joins the first line's wire (a T). Move the new line off the first
   one: the dot goes; undo brings it back. Select the dot alone and press Delete: it is refused
   (the dot is drawn from its lines). Print Book shows the dot too.

9. Draw a line from a device's point into free space and slash it into a cable. Checks lists
   **Core landed at one end only** for the cable (`-W01 core 1 is landed at -K01:2 only.`).
   Draw on from the line's free end to a second point: the core moves onto the new wire and the
   row goes. A file whose stored core place disagrees with its line reports **Core's stored
   place disagrees with the drawing** (an error) and is not changed.

10. Cable types (a cable's type is now its own field, not a property). In **Standards ▸ Cable
    types** add one (cores `4G`, size 1.5). In the cable list pick a cable, choose the type in
    **Cable type**: its cores and size follow the type, and a value typed on the cable wins
    over the type's. Try to delete the type in Standards: it is refused and the message names
    the cables using it (`-W01 uses …`). A cable whose type is not in the project shows
    **Style not found** in Checks, naming the cable.

11. A cable across two pages (step F4). On page 1 draw lines from a device's points into free
    space toward the right, slash them into cable `-W01` and put its reference at their free
    ends (`cablereference`). Add a page, make it active and continue the cable there (`cablecontinue`,
    at the left, toward the right): page 1's lines now end on the reference (no square, no dot)
    and each stub starts on the new reference. Connect each stub's free end to a device point
    on page 2: each core becomes **one wire from page 1's point to page 2's**, in one net, and
    the core moves onto it. The inspector's cable row lists each core's two points
    (`through …`), in Raw and Readable. Erasing page 2's drawing leaves page 1's lines loose
    again, the cores on them. Not yet: some cable-reference views (hover text of a line ending on
    a reference, the debugger trail) still describe the old drawing.

12. Continuations (step F5). On page 1 draw a line from a device's point to a new continuation
    named `T1`; on page 2 another `T1` continuation and a line from it to a device's point. The
    wire list shows **one wire** from page 1's point to page 2's, and both pages label it with
    the same number. Erase page 2's line: the wire goes (page 1's line has no wire again). Undo,
    then rename page 2's continuation to `T2`: the wire goes and both continuations are listed
    under **Continuation without partner**; undo brings the wire back.

13. The demo (`cargo xtask demo`, then open it), against `docs/quick-start.md` §5I:
    - Step 1: page 2 no longer labels the three lines above the contactor (`L1-01 2.5 BN` …):
      their continuations `L1`–`L3` have no partner, so the lines carry no wire. Cable `-W01`
      now continues on page 3: its cores below the contactor show `L1-01`, `L2-01`, `L3-01`
      (number only), and on page 3 the stubs from its reference run to `-X02`, the motor's wires
      showing `L1-01 2.5 BN`, `L2-01 2.5 BK`, `L3-01 2.5 GY`. Each core is one wire from `-K01:2`
      (`:4`, `:6`) to `-X02`.
    - Step 2: the Net line reads `Net 0V-01 · 3 wires · -G01:-, -H01:X2, -H02:X2, -K01:A2`
      (four points, three wires; the continuations are no longer wire ends).
    - Step 3: drawing from the lamp's `X1` to an empty spot makes a line with no wire yet (the
      Checks row **Line not connected at both ends: no wire yet**), not `Wire 24V-04 drawn`. Draw
      on from its free end to a connection point to make the wire.
    - Step 7 and the Checks list: no `-W01` orphan reference, open ends, no-wire-yet or
      core-at-one-end rows any more; ten rows in all.

Not yet tried by the owner. Checked by the headless tests named in the lane's tests
(`lanes::lane_072d`, ecad-model, ecad-file, ecad-inspector, ecad-engine, ecad-mcp, ecad-app).
