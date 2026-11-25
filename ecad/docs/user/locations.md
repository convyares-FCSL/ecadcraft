# Locations and functions

A device's **location** (`+CP1`, `+FIELD`) says where it physically is; its **function** (`=F1`,
`=PUMPS`) says what it is for. These are the IEC 81346 `+` and `=` aspects. Both are stored on the device
and set by you. Boxes, pages and the project only **propose** a value; they never change a device by
themselves.

Words used here are in the [glossary](../reference/glossary.md).

## Set a device's location and function

Select a device. In the inspector's Device tab, type in **Location** (`+CP1`) or **Function** (`=F1`).
A new code is added to the project's list in the same step (`New location +CP1 added to the project's
list.`); one undo removes both. A device with none shows `?`. When the selected symbol sits inside a box,
or on a page with a location, the empty row offers **Use +CP1**.

Parts of a device may have their own location:

- a connector: **Location (socket)** and **Location (plug)**, so each half's BOM row is at its own
  location;
- a device with terminal symbols that are not a strip: **Terminals in**.

An unset part shows the device's value faint, `+CP1 (device)`.

## How a tag shows it

A symbol shows the device's function and location in front of its tag where they differ from the place
the symbol is drawn: a contact of `-K01` (`=F1+CP1`) drawn on a page whose function is `=F2` and location
`+CP2` reads `=F1+CP1-K01`. Where they match, the tag alone shows.

## The project's list

**Settings ▸ Project info** lists the functions and the locations with their device counts; `● default`
marks the project's default.

- Click a code and type a new one (`+CP1` → `+CP10`): every tag, box label and BOM row follows at once.
  A code another entry already has shows red with **Merge into …**.
- The row's menu: **Make default**, **Merge into…** (moves every user, then removes the entry),
  **Remove** (only when nothing uses it).
- **Remove unused (n)** removes every entry nothing uses, in one step.

## Location boxes and function boxes

A location box is a dashed outline labelled `+…` on a page; a function box is dash-dot and labelled
`=…`. A box may carry both. Boxes are drawing aids, not devices. Where boxes nest, the innermost wins.

Draw one from the **Location** group in the tool strip: **Location box** or **Function box**. Type the
code, then click two corners. At the first prompt, `P` switches to a polyline outline (click the points,
then `C` to close) and `S` takes an existing closed polyline. A box smaller than 10 × 10 mm is refused
and the prompt stays open.

Selecting a box's outline or label shows its section in the inspector: **Function**, **Location**,
**Panel temperature (°C)** (used by the [rating checks](circuits.md)), **Style**, **Vertices** and
**Devices inside**. An unset panel temperature or style shows the project's, faint. Box styles are in
Settings ▸ Standards ▸ **Location boxes**.

## Proposals and the conflict list

A box, a page or the project proposes its location (or function) where you place or move a symbol:

- A device with no value takes the proposal at once. Undo removes the symbol and the value together.
- A device that already has another value is never changed silently. It is listed under the intent line
  (`-K05  +CP2 → +CP1`) with **Move all**, **Leave all** and a **Move** per row; its symbols are ringed.
  For a connector half the row also offers **Half**, which sets only that half. Leave is the default:
  Esc, or any other edit, leaves them.
- Drawing a box round existing devices, or typing a new code in a box's Location row, proposes the same
  way.
- A page's own location or function (its page properties) proposes itself for the unset devices on that
  page outside every box.

A value you type is **fixed**; a value taken from a proposal is not. When a box's value changes later,
only devices that took it from a proposal are offered the new one.

## Checks

| Check | Means |
| --- | --- |
| Location used by none of its symbols | `-K01 is +CP2, but none of its symbols is in +CP2`; offers **Use it** when they share one |
| Function used by none of its symbols | The same for a function |
| Symbol tied to multiple locations (or functions) | Error: one symbol sits where two values apply |
| Function or location no longer in the list | A device names an entry the list no longer has |
| Devices without a location / without a function | Counts |
| Location box outline crosses itself | The outline is not a simple shape |
| Location boxes overlap without nesting | Two boxes overlap partly |

## Command window

```
setlocation {"component": "<device id>", "value": "+CP1"}
setfunction {"component": "<device id>", "value": "=F1"}
locationbox {"location": "+JB2", "corners": [[110, 60], [150, 90]]}
functionbox {"function": "=F1", "points": [[x, y], ...]}
setprop {"aids": ["<aid>"], "key": "location", "value": "+CP2"}
setprop {"aids": ["<aid>"], "key": "panel_temperature", "value": "45"}
pageprops {"location": "+FIELD"}
conflicts {}
conflicts {"answer": "move"}
designations {}
designationedit {"aspect": "location", "code": "+CP1", "to": "+CP10"}
```

`locationbox` and `functionbox` also take `item` (a closed polyline to take the outline from), `page` and
`proposal` (`propose`, the default, `move` or `keep`). `place`, `pageprops` and `projectprops` take
`location` and `function` too. JSON calls never prompt: `conflicts {}` lists what waits.

## Limits

- **Function and location box** and **Device or PLC outline** are greyed in the Location group: not
  available yet.
- **Show devices** in a list entry's menu is not available yet.
