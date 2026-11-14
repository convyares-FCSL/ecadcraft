# Brief 050a: the style model (standards and style catalogues)

Part of the [quick start](../quick-start.md). The Standards screen of brief 042 is described in
[042-standards.md](042-standards.md) where that page exists; this page says what 050a changed.

## What this brief changed

**Branch `feature/style-model` (brief 050a)**, cut from `dev` at `be6f4413`.

| Where | What is there now |
| --- | --- |
| Standard | A project uses at most one **standard**, a named house style it holds a copy of: values for any registry default at the Standard level, and its own styles. Without one it uses **IEC (built in)**. `standard {new}`, `{name}`, `{from: <other .ecad>}` (copies that project's standard, keeping its ID; the other file is opened read-only) and `{remove: true}` (back to IEC, one undoable step) |
| Style catalogues | **Text styles** (one built-in style per text kind, with today's cap heights and weights, plus a new **Colour**), **Location boxes** (line type, weight, colour, label style; fill is reserved, stored and drawn as none) and **Frames** (border and inner weights, line colour, grid labels). A standard or the project adds its own styles or overrides fields of built-in ones (`setstyle`, `deletestyle`, `promote`, read with `styles`) |
| Choosing a style | Registry entries `text.<kind>.style`, `location.style` and `print.frame.style` say which style is used: a device may choose its own text style, a location box its own box style, a page its own frame style. Settings ▸ Standards ▸ Defaults lists them under **Styles** |
| Settings ▸ Standards | The header says the standard (**Make a standard…**, **Rename**, **Copy from project…**, **Back to IEC**), each saying its consequence before anything is written. The strip reads **Defaults · Text styles · Location boxes · Frames**. A style row's detail shows each field's chain (Built-in → Standard → Project), an editor at Project or Standard level, Reset per level, **New style from this**, **Rename**, **Delete…** (with a replacement when it is used) and **Promote to standard**. **Used by** is filled for the selected row |
| Inspector | A selected device shows a **Style** row per text kind (a list of the text styles); a location box shows **Box style**; with nothing selected, the page shows **Frame style**. **Reset** clears the object's or page's own choice. A choice naming a style that no longer exists reads `Not found: drawn as Tag` |
| Status bar | `Standard: <name>` |
| Checks | `style-unknown` (a choice naming no style), `ignored-old-setting` and `style-limits` (records past the bounds of a hostile file) |
| Prints | A style's colour prints in a colour print; a mono print is ink, as before |

**Text sizes set before 050 go back to the built-ins** (FP4, no conversion): 031's project
properties `text.<kind>.height`, `.weight` and `.font` are no longer read. Checks lists one
`ignored-old-setting` row per kind of key ("Text heights set in an older build, now ignored: set
them in Standards"); set them again in Settings ▸ Standards ▸ Text styles, then clear the old key
with `projectprops {"props": {"text.tag.height": null}}` to remove the row.

Known limits in 050a: a device's own text style changes the height, weight and colour of its
texts; the font stays the kind's project style's. A wire's own `text.wire_label.style` is stored
and resolved but wire labels are drawn in the project's wire label style. An agent cannot run
`standard {from}` (it names another file); a person can, at the command line or in the header.

## Try it

1. Open the demo, then **Settings ▸ Standards**. Expected: the header reads `STANDARD  IEC (built
   in)` with **Make a standard…**; the strip shows Defaults, Text styles, Location boxes, Frames.
2. **Make a standard…**, type `House`, Enter. Expected: the header reads `House · based on IEC ·
   0 values · 0 styles`; the status bar says `Standard: House`; History has `Make standard House`.
3. **Text styles**, click **Tag**. In the detail, type `RD` in **Colour**: the consequence line
   says how many use Tag and `Colour none → RD`. **Apply**. Expected: every tag on the page turns
   red; the row's Layer badge reads `Built in •`; one step in History.
4. **New style from this**, name `Tag large`, **Add**; set its **Cap height** to `5`, **Apply**.
   Close Settings, select one coil: the inspector's **Tag style** row lists `Tag large`; choose it.
   Expected: only that coil's tag is larger.
5. Back in Settings, **Tag large ▸ Delete…**. Expected: `1 device use Tag large. Replace with:`
   and a list; choose **Tag**, **Delete**: the tag is normal again, in one step.
6. **Copy from project…**: type the path of another `.ecad` that has a standard, **Read**. The
   consequence line says `Replace House with … (n values, m styles)? Your project values stay.`;
   **Copy**. The other file is not changed.
7. **Back to IEC**: the line says how many values and styles go back; confirm. `u` brings the
   standard back.
8. Print one page with **colour: mono** and one in colour (Print Book): the red tags print black
   in mono and red in colour.

## Commands

```text
standard {}                                    read
standard {"new": "House"}
setstyle {"catalogue": "text", "id": "tag", "fields": {"colour": "RD", "height": 3}}
setstyle {"catalogue": "text", "new": true, "name": "Tag large", "copy_of": "tag", "fields": {"height": 5}}
setdefault {"key": "text.tag.style", "level": "object", "component": "<id>", "value": "Tag large"}
deletestyle {"catalogue": "text", "name": "Tag large", "replace_with": "tag"}
promote {"catalogue": "text", "name": "Tag large"}
setdefault {"key": "symbol.coil.prefix", "level": "standard", "value": "Q"}
styles {"catalogue": "location", "users": true}
```

## Checked here

Automated: `cargo test -p ecad-model lane_050`, `-p ecad-file lane_050`, `-p ecad-cadcraft
lane_050`, `-p ecad-mcp lane_050`, `-p ecad-trial lane_050`. Not owner-tried yet.
