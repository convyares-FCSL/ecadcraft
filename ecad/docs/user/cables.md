# Cables

A **cable** (`-W01`) groups wires that run together; each wire is one of its **cores**. You make
one by drawing a short line, the **slash**, across the wires, or build it first in the cable
manager and assign wires to it later. The wires themselves do not change. Continuing a cable on
another page is in [References](references.md); cables on terminal strips and the cable schedule
are in [Terminal strips](terminal-strips.md) and [Reports](reports.md).

## Make a cable with a slash

1. Press **C** (or **Cable ▸ Cable slash**). The prompt says
   `Specify first point of cable slash or [Cable]:`.
2. Click on one side of the wires and move across them. Before the second click a ring marks each
   wire the slash takes and the intent line counts them (`-> 3 wires, 1 other object ignored`).
   Nothing is written yet.
3. Click. The cable is made with those wires as its cores; the slash stays, in the wire colour,
   with a small dot on each wire it holds.

New cables are tagged `-W01`, `-W02`… (devices and cables share one set of tags). Undo removes the
slash, its dots and the cable in one step.

**What the slash takes.** A wire counts when the slash crosses it, passes through one of its
bends, or ends on it. A wire the slash meets only at the wire's own end, or runs along, is not
taken; the reply says how many were left out. Other lines it crosses are ignored and reported;
arcs, circles, text and symbols are never taken. The cores are ordered by where the slash crosses
them, from its first point; the order means nothing else. A line with no wire yet can be slashed
too: its core moves onto the wire when the line is finished.

**One cable per wire.** A slash across a wire already in a cable is refused: `Wire … is already in
cable …; a wire can be in only one cable.` The prompt stays, so you can try another point.

**An existing cable.** Type `C` (Cable) at the prompt, then a cable's tag, or Enter for **New**:
the slash then assigns that cable. While the tool runs, the inspector lists **New cable**, the
cables not drawn yet, and the drawn cables not slashed on this page. A slash with a drawn cable's
tag on another page shows that cable there, over its own wires; on a page where none of its wires
runs it is refused.

Typed: `cableslash {"from":[28,-3],"to":[32,18]}`, with `"tag":"-W1"` to name it or
`"cable":"-W01"` to assign an existing cable.

## Work with a cable

Right-click the slash for the cable's wheel:

| Slot | Does |
| --- | --- |
| **Cable view** (top) | Switches between traditional and fan ([below](#traditional-and-fan-view)) |
| **Continue** | Continues the cable on another page ([References](references.md)) |
| **Go to reference** | Goes to the cable's other end on another page |
| **Show row** | Opens its row in Project ▸ Cables |
| **Delete** (bottom) | Deletes the cable; its wires stay |
| **Join** | Moves where the fan's trunk joins an end |
| **Core labels** | Gives each core a number or colour |
| **Add reference** | Marks where the cable leaves the page |

- **Add a wire:** the wire wheel's **Add to cable**, or `cableadd`. A new member takes the next
  spare core of the cable's core offer.
- **Delete:** erasing the slash removes only the drawing; the cable stays. The wheel's **Delete**
  deletes the cable itself: `Cable … deleted; its 3 wires are kept.`
- **Hover** the slash: the cable's tag, view and wires. On a wire: `In cable …`, with its core.
- Name and type the cable in the inspector, or with `setprop {"cable":"<id>","key":"tag","value":"-W5"}`.

## Core labels

The cable wheel's **Core labels** (or Ctrl+K **Core label**): click a wire of the cable and type
its core number or colour (`1`, `BK`), Enter; the prompt moves on to the next wire. Enter with
nothing typed leaves a label as it was; a single `.` clears it. `u` undoes one label at a time.

A core label is free text of at most 32 characters; duplicates in one cable are allowed. ECAD never
numbers cores itself or checks them against colours. In traditional view a label sits just past
the slash; in fan view at the start of each tail, before the wire number.

Typed: `cablecore {"conductor":"<id>","core":"1"}` (`null` or `""` clears);
`cablecore {"cable":"<id>","cores":["1","2",null]}` sets every core at once, in member order.

## Traditional and fan view

A cable is shown **traditional** (every wire, and the slash) or as a **fan**: a label box (tag,
designation, type) on a thick trunk that joins a thick spine at each end, a curved bend where each
wire leaves the spine, and a thin tail to each wire end with its core label and wire number.

- Switch with the cable wheel's **Cable view**, or Ctrl+K **Cable view** and a click on the cable.
  With several objects selected it switches every cable they name: if all are fans they all go
  back, otherwise they all become fans.
- The view changes only what is drawn: nothing is written to the project, the change count does
  not move, and undo skips it.
- The fan is ordinary drawing content on the reserved layer `ECAD-CABLES`, in magenta: it snaps,
  selects, prints and exports as shown. Drawing new objects on that layer is refused.
- Editing the fan itself is refused: `This cable is shown in fan view; switch to traditional view
  to edit.` Moving a device whose connection points have fan wires attached is allowed; the fan
  follows.
- **Join** (the wheel's slot, or Ctrl+K **Cable join**): click the fan near an end, then where the
  trunk should join that end's spine; **Middle** puts it back. The join stays on the spine between
  the outermost wires.
- Thick and thin show with lineweights on, which ECAD turns on when it opens or creates a project.
- A cable of one wire is not offered a fan. A fan cable with only one of its wires on a page is
  drawn traditional there. At a reference end the fan stops at the reference's flag.

Typed: `cableview {"cable":"<id>"}` switches one cable (`"view":"fan"` or `"traditional"` sets
it); `cableview {}` lists every cable's view. `cablefan {"cable":"<id>","end":"A","at":[x,y]}`
moves a join (`"at":0.25` a quarter of the way along, `"at":null` the middle).

### Remembered views

Views are remembered per person and per project, outside the `.ecad` file, in `cable-views.json`:
`%APPDATA%\ECAD\` on Windows, `$XDG_CONFIG_HOME/ecad/` or `~/.config/ecad/` on Linux. The
environment variable `ECAD_SETTINGS_DIR` names another folder. Someone else opening the file sees
every cable traditional.

- If the file cannot be read, every cable opens traditional and ECAD says so; if it cannot be
  written, the switch still happens for the session.
- The file is never read past 1 MiB, and at most 25 000 cables of one project are remembered in
  fan view.
- The same project in two windows: the window that switches last decides what is remembered.

## The cable manager

**Project ▸ Cables** lists every cable, drawn or not, and builds cables before they are drawn.

1. **+ Cable** adds a new `-W` row, badge `not drawn`, and opens the **Details** pane with the
   cursor in **Tag**. The foot counts the cables not drawn.
2. In Details set **Cable type**, **Cores** (`4G`, `7`, `BN,BK,GY,BU,GNYE`), **Shield**,
   **Length**, and each end's planned **Function**, **Location** and **Gland**. Each value is one
   undo step; a bad value turns the box red with the reason, and nothing is written. From and To
   show the plan (`+FIELD (planned)`).
3. **Add a core** raises the core offer by one (`4G` → `5G`). Reducing the cores below those in use
   is refused, naming them.
4. **Assign in the drawing…** (Details, the **+ Cable** menu, or the Checks row) switches to Draw
   on the page whose location matches the planned cabinet end and starts the slash with that cable
   picked. The crossed wires become its cores in order (`-> 3 wires onto -W03 (1, 2, 3)`); more
   wires than free cores is refused. Esc writes nothing.
5. **Remove** (foot) removes a cable that is not drawn. A drawn cable is refused with the pages it
   is drawn on (`-W01 is drawn on 2/D1. Delete it on its page.`).

**Glands.** Each end's gland comes from the cable's own **Gland** box, else its cable type's gland
(**Settings ▸ Standards ▸ Cable types**), shown as `from type` or `set here`. The list's
**Glands** column reads `M20, M25`.

**Cable types.** A cable's type gives its cores and size; a value typed on the cable wins over the
type's. A type in use cannot be deleted. See [Styles](styles.md).

**Reports ▸ Cables** has **Include not-drawn cables** (on by default). Typed:
`cablelist {"include_not_drawn": false}`.

Typed: `cablenew {"cores":"4G","to":{"location":"+FIELD","gland":"M20"}}`,
`cableedit {"cable":"-W01","set":{"length":12,"shield":"overall"}}` (`null` clears a field),
`cableremove {"cable":"-W01"}`.

## Checks about cables

Among the rows ([Checks](checks.md)): **Two cables share a tag**, **More cores used than the cable
offers**, **Core used by two wires**, **Cables without a core count**, **Cable reference with no
other end**, a cable not in the drawing yet (Info), a core not assigned, a core landed at one end
only, a drawn end with no gland size (with **Set gland**), and **Style not found** for a cable whose
type is not in the project.

## Limits

- Only straight two-point slashes; one cable per slash.
- Moving a wire does not change the cables it is in; if the slash no longer crosses it, only its
  dot goes, with no warning. Deleting a cable's last wire leaves an empty cable.
- The fan's tails (at least 20) and bends (2.5) are fixed drawing units; the trunk's corners are
  sharp.
- The **Gland** box is free text, not a list of sizes.
- **Add to cable** has no cable picker of its own; it follows the next-spare-core rule.
