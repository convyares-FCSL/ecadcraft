# 072 Typed object model: lane F (drawing aids)

What changed for someone using the app:

- **A continuation arrow is a drawing aid, not a device.** It has no tag, never appears in the
  device list, the BOM or numbering, and is placed, moved, erased and undone as before.
- **Selecting an arrow shows a Continuation section** in the right panel: Name, Caption and
  Other ends (with Go to buttons). Typing a new name or caption there is one undo step, and the
  section shows the new value at once.
- **Checks on an arrow point at its connection point**: an unknown name ("Continuation name not
  a potential") and a lonely arrow now carry the arrow's page and grid place, so the row jumps to
  it.
- **Older files with an arrow stored as a device open read-only**, naming the old
  `continuation` device form (pre-v1: no conversion).
- Agents and the command line rename or caption an arrow with
  `setprop {"aids": [<aid>], "key": "name" | "caption", "value": …}`; `place {"symbol":
  "continuation"}` answers `aid` (no `component`); `xrefs {"aid": …}` lists its other ends.
- **A location box (and a function box) is a drawing aid too.** It is drawn, moved, erased and
  undone as before, and the devices inside are proposed its values as before. Selecting its
  outline or label shows the **Location box** section (Function, Location, Vertices, Devices
  inside); a new Function or Location there is one undo step and shows at once. A box no longer
  has a style of its own: every box is drawn in the project's location style. `locationbox` and
  `functionbox` answer `aid` (no `component`); agents set a box's values with
  `setprop {"aids": [<aid>], "key": "location" | "function" | "panel_temperature", "value": …}`
  (`setlocation {component}` is for devices only), and `rating {location, temperature}` still
  sets the panel temperature of every box of a location. Older files with a box stored as a
  device open read-only, naming the old `location_box` device form.

## Try it

1. Place two continuation arrows named `24V` on different pages (the continuation symbol in the place picker).
2. Click one: the right panel shows **Continuation** with Name `24V` and the other arrow under
   Other ends.
3. Change Name to `0V`: the section shows `0V`; the Checks tab lists both arrows as lonely,
   each with its page and place. Undo brings `24V` back.
4. Give it a caption: it is drawn under the arrow and shown in the section.
5. Draw a location box `+CP1` round a coil, then click the box's outline: the right panel shows
   **Location box +CP1**. Change Location to `+CP2`: the label reads `+CP2` at once, the coil is
   asked about (it was `+CP1`), and one undo brings `+CP1` back.

Checked so far: automated headless tests only
(`lanes::lane_072f::renaming_or_captioning_a_shown_arrow_refreshes_its_section` and
`lanes::lane_072f::a_new_location_on_a_shown_box_refreshes_its_section` in ecad-app).
Not yet tried by the owner.
