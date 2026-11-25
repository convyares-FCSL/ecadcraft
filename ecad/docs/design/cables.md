# Cables: why they work the way they do

A cable (`-W01`) is a group of conductors that run together. This note says what the model stores
for a cable, what it only draws, and why. How to make and edit cables is in
[`../user/cables.md`](../user/cables.md); how a cable continues on another page is in
[`references.md`](references.md).

## A cable groups wires that already exist

A cable does not create conductors. It groups wires the designer has already drawn, because the
schematic is drawn wire by wire and the cable is decided afterwards (or planned first and assigned
later). So the cable is its own object, `Cable`, beside the wires it groups, and never a kind of
device: it is a different thing with its own tag, cores and cable type. Cable and device tags share
one namespace.

`Cable` stores its tag (a tag slot, like a device's), its description, its **cores** in core order,
its cable type, and its own shield, rating inputs, design current and protecting device where they
differ from the type's. Everything else a person sees about a cable (what the trunk looks like, where
it continues, how many cores are spare) is worked out from these and from the wires.

## Cores are stored, whole

A cable's cores are its whole core set, stored, including the cores no line carries yet. Each core
has a label (`1`, `BN`, `GNYE`), optional colours of its own (otherwise taken from the cable type by
core order), a **purpose** and a **position**, kept apart:

- Purpose: **in use** (meant to carry a conductor) or **spare** (ticked *Not used*). An in-use core
  left unplaced is an open core, shown on the Checks list.
- Position: **unplaced**, on a drawn **line** that carries no wire yet, or on a **wire**. The command
  that lands a line's second end moves its cores from the line to the wire it forms.

Why stored and not worked out from the connections: a person plans a cable before it is drawn, ticks
cores as spare, and lands a core on a line before both ends are attached. None of that can be read
back off the drawing, and a core that moved when a wire was redrawn would lose the designer's choice.

The cable type gives the **core offer** (`7G1.5` → seven cores, one green-yellow). A wire added to a
cable takes the next free core of the offer. A cable never gains more wires than it has cores, and its
cores cannot be reduced below those in use: both are refused, naming the cores, with nothing written.
With no offer, cores are labelled `1…n`.

## The slash: one gesture, one rule

A cable is made by a **slash**: two clicks across existing wires (`cableslash`). One function
decides which conductors a slash takes, and the preview, the interactive commit and the JSON form all
call it, so what is previewed is what is committed.

- Only routes on the active page are tested, with the project's tolerance.
- A wire counts when the slash crosses the inside of a route segment, passes through a bend, or ends
  on the inside of a route.
- Touching a route only at its own end point, or running along a segment, does not count: it is
  reported as "touched", because a slash that grazes a wire end at a connection point is almost
  never meant to take that wire.
- A wire crossed twice counts once. Lines that are not wires are ignored and reported, so the person
  knows why the count is smaller than they expected.
- Counted wires join in crossing order (distance from the first click along the slash; ties by ID).
  That order is the member order and nothing more.

A slash across a cable that is **not drawn** (built in the cable list, no wires yet) assigns the
crossed wires to it. A slash across wires of a cable that is drawn continues it. A wire already in a
cable cannot join a second one.

A slash is stored as a drawn representation of the cable (role slash); it is the only thing on the
page that says "these wires are one cable".

## Traditional and fan view are presentation only

Each cable can be shown **traditional** (every wire drawn, the cable shown by its slash) or **fan**
(one trunk that fans out to the wire ends). The switch (`cableview`) is presentation:

- It writes no transaction, never moves the head, never marks the project unsaved, and undo skips it.
- It is per cable and per user. The choice is remembered outside the `.ecad` file, in
  `cable-views.json` in the user's ECAD settings folder (`ECAD_SETTINGS_DIR` overrides the folder).
  Only fan cables are listed; traditional is the absence of an entry. An unreadable or unwritable
  file never fails an open, a save or a switch: views then start traditional or last for the session.
- The fan is computed, never stored: every time a page is drawn it is laid out from the cable's
  members and their current routes, so editing a route in traditional view and switching back shows
  the fan following it.
- The fan drawing is read-only. An edit or pick on it is refused with "This cable is shown in fan
  view; switch to traditional view to edit." Selecting it is allowed, so hover and the debugger work.
- Programmatic calls ignore views. JSON forms, the control channel and MCP act on IDs and behave the
  same in either view, so scripts and agents never depend on someone's display setting.

Why per user and outside the file: two people reading the same project want different views, and a
view is not a design decision. Keeping it out of the history keeps the history about the design.

A cable of one wire is not offered fan view: there is nothing to fan.

## How the fan is laid out

The layout lives in `ecad-electrical` (`cables/layout.rs`), computed over the document; the engine
only turns its shapes into entities. The reasons behind its shape:

- **Thick is cable, thin is wire.** The trunk and spines use the cable line weight; the bends and
  tails use the wire weight. The eye reads the cable section and the conductors apart.
- **Wire ends never move.** Every tail ends at its wire's real end, so connection point marks and
  attached dots stay where they are and the fan never suggests a different connection.
- **Axis-snapped and straight.** Each fanned end has a spine parallel to the line of wire ends and a
  straight tail per core, entered through a quarter bend capped at half the core pitch, so close
  cores never overlap.
- **A label box on the trunk** shows the tag (or `?` for an unnamed cable, so it shows on a plot),
  then the designation and the type when present.
- **Labels at the tail start.** In fan view each tail shows its core label and then its wire number,
  separated by a gap, reading left to right or bottom to top. In traditional view the core label sits
  just past the slash crossing, along the wire.
- **Hostile values never break a page.** Any non-finite value makes the fan fall back to the
  traditional drawing on that page; long text is shortened for display only.
- Sizes come from the cable text style's height; the cable look has no text size of its own.

Core labels are free text. ECAD does not check them against colours.

## Reference ends

Where a cable's drawing stops on a page and continues elsewhere, a **reference end** stops it. In fan
view that side does not fan: the trunk runs to the reference flag. What a reference end says is
derived every time the page is drawn, never stored: see [`references.md`](references.md). Each core
also stores its points on the cable's reference ends, so a line landing on one page's point and a line
on the other page's are one net, and the wire report runs from device to device across the pages.

## Planned cables

A cable can be built in the cable list before it is drawn. It is an ordinary cable, shown as **not
drawn**, never as an error. Each end has its own planned location and function: a cable spans two
places, so the two ends differing is normal; a check fires only when a drawn end differs from that
same end's plan. Glands, maker and part number default from the cable type and may be overridden per
cable or per end. A drawn cable cannot be removed from the list; the refusal offers a go-to for each
page it is drawn on.
