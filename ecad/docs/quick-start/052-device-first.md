# 052: the device-first model

Brief 052, design `docs/design/052.md`. This file grows as parts a to d land.

## Part a: symbol types, and no `main` symbol

Every symbol of a device is equal. What a symbol is for its device (head, contact, pole,
terminal, channel, half, pin) comes from its definition, never from the order it was placed in.
A device's **head** is its coil, actuator, breaker or PLC card body, by type.

Nothing new appears on screen. What changes is which symbol a rule follows when it was not placed
first.

### Try it

1. **A coil placed last is the head.** Place an NO contact (Device ▸ Symbol ▸ NO) on page 1.
   Join an NC and another NO to the same device. Add a page and join a coil to that device. Each contact's reference text now points at the coil's page,
   and the contact table is drawn under the coil.
2. **Joining a second coil is refused.** Place another coil on the same device: `-K… already
   has a coil.`
3. **Numbering follows the head.** With provisional tags on (Project ▸ Numbering), place a
   contact of a new device at the left of page 1, a coil of another device to its right, and
   the first device's coil on page 2. Renumber devices: the device whose coil is on page 1
   gets the lower number.
4. **Pin-only connector halves.** Place a socket pin (`pins`) on page 1 and its plug pin
   (`pinp`) on page 2. Each pin shows its mate's place; there is no half-level reference text
   beside the first pin.

## Part b: function and location stored on the device, and the project's list

A device's location (`+`) and function (`=`) are what you set on it. Boxes, pages and the project
no longer give a device a location by themselves (part d adds the proposals). Each location and
function is an entry in the project's list, so renaming one renames it everywhere.

Projects from v0.2.0 or earlier: their locations and functions show `?` and Checks lists them as
no longer in the list. Re-make them; nothing converts them.

### Try it

1. **Set a location.** Select a device; in the inspector, type `+CP1` in Location. The echo says
   `New location +CP1 added to the project's list.` One undo removes both.
2. **No location is `?`.** A device with none shows `?`. With a symbol selected inside a box or on
   a page with a location, the row offers **Use +CP1**.
3. **Prefixes.** Give `-K01` `=F1` and `+CP1`. A contact of it on a page whose function is `=F2`
   and location `+CP2` shows `=F1+CP1-K01`.
4. **Connector halves.** Select a connector: **Location (socket)** and **Location (plug)** each
   take their own value; the BOM has one row per half at its location.
5. **Checks.** A device whose location none of its symbols is in shows
   `-K01 is +CP2, but none of its symbols is in +CP2`, with **Use it** when they share one.

## Part c: Settings ▸ Project info ▸ Functions and locations

1. Open Settings ▸ Project info. The card lists functions and locations with their device counts;
   ● marks the project's default.
2. Click a code and type a new one (`+CP1` → `+CP10`): every tag, box label and BOM row follows at
   once; `u` undoes it. Typing a code another entry has shows it in red with **Merge into …**.
3. The row's menu (⋯): **Make default**, **Merge into…** (moves every user, then removes the entry),
   **Remove** (only when nothing uses it).
4. **Remove unused (n)** removes every entry nothing uses, in one step.

## Part d: proposals, the conflict list and function boxes

A box, a page or the project **proposes** a location (or function) where you place or move a
symbol. A device with none takes it; a device with another value is listed and never changed
silently. Leave is always the default.

### Try it

1. **Place a contactor in a box.** Draw a location box `+CP1`, then place a coil inside it: it is
   `+CP1` at once. One undo removes the coil and its location together.
2. **Draw a box around an existing device.** Devices with no location inside take the box's; a
   device with another location is listed under the intent line: `-K05  +CP2 → +CP1`, with
   **Move all**, **Leave all** and a **Move** per row. Their symbols on the page are ringed; the
   row under the pointer rings its device in the warning colour. Esc (or any other edit) leaves
   them.
3a. **A connector half elsewhere.** Place a plug pin of a `+CP1` connector inside a `+FIELD`
   box: its row also offers **Half**, which gives only the plug `+FIELD`.
3. **Move a contact into another box and leave it.** Drag an unset device's only symbol into box
   `+CP1`: it is set (the echo says so). `u` clears the location; `u` again moves it back. Move a
   device that already has `+CP2`: it is listed, and Leave keeps `+CP2` (its symbol there shows
   `+CP2-K05`).
4. **Draw a function box.** Location ▸ Function box, type `=F1`, draw it around a location box.
   It is dash-dot and labelled `=F1`. A device placed inside takes both `=F1` and `+CP1`.
5. **Give a box another entry.** Type `+CP3` in a box's Location row: devices inside at `+CP1`
   are listed; devices inside at another value are not.
6. **Page and project values.** A new page location proposes itself for the unset devices on that
   page outside every box.

For agents: `place`, `pageprops` and `projectprops` take `location` / `function` (on `place`) or
`proposal` (on boxes, pages and the project): `propose` (default), `move`, `keep`, or a code on
`place`. JSON calls never prompt; `conflicts {}` lists what waits, `conflicts {answer: "move"}`
moves it.

### Automated checks

- `cargo test -p ecad-engine --test all lane_052`
- `cargo test -p ecad-model --test all lane_052`
- `cargo test -p ecad-mcp --test all lane_052`
- `cargo test -p ecad-app --test all lane_052`
