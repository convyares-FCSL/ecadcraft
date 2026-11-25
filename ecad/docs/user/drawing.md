# Drawing

How to draw on a page: the tool strip, the wheel, picking, the keys, the canvas and the command
window. What each tool makes is in its own guide ([Devices](devices.md), [Wires](wires.md),
[Cables](cables.md), [Terminal strips](terminal-strips.md), [Locations](locations.md)).

## The tool strip

The strip at the left edge of the **Draw** workspace holds, top to bottom:

- the **Book**, **Devices** and **History** panel buttons (see [Workspace](workspace.md));
- the tool groups: **Select**, **Draw**, **Text** | **Device**, **Terminal**, **PLC** | **Wire**,
  **Cable** | **Location**;
- the **Snap**, **Grid** and **Ortho** toggles.

Hover a group to open its flyout: the group's tools, the last-used one lit. A click on the group
starts its last-used tool. A greyed item is a tool that does not exist yet; its tooltip says so.

| Group | Key | Tools |
| --- | --- | --- |
| Select | V | Select |
| Draw | D | Line, Polyline, Rectangle, Circle, Arc |
| Text | T | Text, Note, Revision cloud |
| Device | – | Symbol, Connector |
| Terminal | – | Terminal, Terminals across wires, Draw a jumper bar, Renumber…; the Terminal manager at the foot (terminal blocks and strips) |
| PLC | – | Rack overview, Detail page…, Card…, Channel… |
| Wire | W | Wire, Bridge, Continuation |
| Cable | C | Cable slash, Continue cable |
| Location | – | Location box, Function box |

A group key starts the group's last-used tool; pressed again while that tool runs, it steps to
the group's next tool.

**Snap, Grid and Ortho.** A click flips the mode. Hover for its popout: Snap spacing (0.5, 1,
2.5 or 5 mm), Grid shown or hidden with its spacing and major lines, Ortho 90° only, 45° steps or
free. ECAD's own tools always snap to connection points.

## Tools and prompts

A running tool prompts in the **intent line** at the top of the canvas: the prompt, its options
as chips you can click, what you are typing, and, for Wire and Connect, what the click will join
("joins A1 of -K01"). It shows only while a tool prompts or a refusal waits; a refusal says what
was refused, why, and offers **Show it**.

- Answer by clicking, or type a point (`40,10`, `@40,0`) or an option's first letter and press
  Enter. Typing goes to the running prompt.
- Enter, Space or a right-click finishes the step; Esc cancels what is not done yet. While a tool
  runs, a right-click is Enter and opens no wheel.
- Ctrl+Z inside a tool takes back its last point (for example the last bend of a wire).

## The wheel

With no tool running, right-click an object on the page: its wheel opens round the cursor with a
chip naming it. Click a slot, or press, move and release over one. Esc, or a click or release
outside, closes it. The slot under the cursor names itself in a tooltip; a greyed slot says why
and may offer the next step.

Slots mean the same on every wheel: bottom deletes, right goes to what the object relates to,
bottom right shows its row in the Project workspace, and the three on the left copy, turn or
change it. An empty slot stays empty.

| Object | Top | Top right | Right | Bottom right | Bottom | Bottom left | Left | Top left |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Device | Edit tag | Properties | Go to contacts | Show row | Delete device | Duplicate | Rotate | Edit symbol (Make symbol on a drawn device) |
| Wire | Fix number | Add to cable | Show net | – | Delete | Set potential | Join | Wire settings |
| Wire end | Connect | Disconnect | Show net | – | Delete | – | – | – |
| Cable | Cable view | Continue | Go to reference | Show row | Delete | Join | Core labels | Add reference |
| Several objects | Move | Copy | Make symbol | – | Delete | Duplicate | Rotate | Mirror |
| Empty page | Paste | Page properties | – | Show page in Pages | – | – | New page | – |

Text, terminal blocks and references have wheels of their own; a page button in the bottom bar or
a page row in Book opens the page's wheel. See the guide for each.

## Picking and selecting

Click an object to select it; drag a window to select several. With no tool running, Esc clears
the selection. Selecting a symbol of a device draws a dashed outline round the device's other
symbols on the same page.

Rest the pointer on an object with no tool running: a tooltip names it (a device's tag and
type, a connection point and its wire, a wire's number, net and ends, a cable's view and cores).
Hovering never changes anything.

## Move, turn, copy, delete

Keys act on the selection while the pointer is over the page and nothing prompts:

| Key | Does |
| --- | --- |
| Del | Delete |
| R | Rotate 90° |
| X | Mirror |
| Ctrl+C, Ctrl+V | Copy, paste |
| Ctrl+D | Duplicate |
| F2 | Edit the selected tag |
| Ctrl+Z, Ctrl+Y (or Ctrl+Shift+Z) | Undo, redo; also while a tool runs |

To move, select the objects and right-click for **Move** (the wheel of several objects), or type
`move` in the command window and give a base point and a second point. A wire attached to a moved
connection point follows it.

## Other keys

| Key | Does |
| --- | --- |
| Ctrl+1 … Ctrl+4 | Draw, Project, Library, Publish |
| Ctrl+K | Search pages, tags, symbols and actions; Enter or a click runs an action on the selection or starts its tool |
| Ctrl+/ | Open or close the command window (the Text panel) |
| PgUp, PgDn | The previous or next page |
| Alt+Left, Alt+Right | Back and forward over pages and workspaces |
| F | Fit the drawing |
| ] | Fold or open the inspector |
| Esc | Close the wheel or a flyout; clear the selection |

Single-letter keys count only while the pointer is over the page and no text field has the
keyboard. **Settings ▸ Shortcuts** lists them.

## The canvas

- **The sheet.** A page opens fitted, frame and title block in view. After that it keeps the zoom
  you leave it at. The bottom bar has **Fit**, zoom out, the zoom % and zoom in.
- **Zones.** The frame numbers its columns across the top and letters its rows down the sides.
  References read page, then zone (`2/B5`); see [Pages](pages.md) for other styles.
- **Sheet colour.** The bottom bar's sheet colour menu: **Paper** (always light), **Follow**
  (dark in the dark interface, paper in the light one) or **Dark**. It changes the screen only:
  PDFs and prints always use paper colours. The sun and moon in the top bar switch the interface
  between light and dark. Both are kept with your window layout, not in the project.
- **Markers.** Drawn over the page at a fixed screen size and never printed: a connection point
  is a ring, an attached wire end a green dot, a loose end a grey square, a warning a triangle
  with "!", an error a disc with "!", and a provisional tag a dotted underline.
- **Wire colours.** A wire with a colour is drawn in it; a two-colour code (`GNYE`) draws the
  first colour with dashes of the second. See [Wires](wires.md).
- **Page text** uses Barlow Semi Condensed; heights are cap heights.

## The command window

**Ctrl+/** opens the Text panel over the inspector. Its **Commands** tab holds the command window:
the **ECAD command** field, **Run**, and the reply under it, plus the read-only developer tools.

- Type a command's name alone (`wire`, `addconnectionpoint`) to start its prompts, as its tool
  does.
- Type `name {json}` to run its JSON form with no prompts, as scripts and agents do:
  `wire {"points":[[0,0],[40,0]]}`.
- Plain drawing commands work too (`line 0,0 40,0`, `move`, `u`, `redo`).
- Each command's parameters are described in [Agent commands](../agents/commands.md).
- Some actions (Set property, Add to cable, Delete electrical object, Rename pin…) put an editable
  line here with the selected object's ID filled in. Replace each `<placeholder>`; a line that
  still holds one is refused, never run.

`ui.sheet paper|follow|dark` sets the sheet colour from here.

## Limits

- Rotate by mouse and key turns in steps of 90°; other angles go through the JSON form
  (`"rotation"` on `place`).
- Tab does not cycle through connection points under the cursor. A typed point attaches only when
  it is exactly a connection point's point.
- ECAD objects are drawn in model space only; ECAD tools do not run in a paper-space layout.
- Markers are drawn only in the main drawing area.
- Grid dots are painted over the drawing, so a dot can sit on a wire.
- CADCraft's own interface (menus, Tool Sets, command line) is hidden. **Settings ▸ Appearance**
  has the developer switch that shows it; see [the developer guides](../developer/architecture.md).
