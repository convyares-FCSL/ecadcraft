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
  outline or label shows the **Location box** section (Function, Location, Panel temperature
  (°C), Style, Vertices, Devices inside); a new value there is one undo step and shows at once.
  An unset Panel temperature or Style shows the project's, faint. A box keeps a location style of
  its own (Style): it is drawn in it, counted under the style's **Used by**, and moved to the
  replacement when the style is deleted; an own style that no longer exists falls back to the
  project's and is listed in Checks. `locationbox` and `functionbox` answer `aid` (no
  `component`); agents set a box's values with `setprop {"aids": [<aid>], "key": "location" |
  "function" | "panel_temperature" | "style", "value": …}` (null clears the temperature or the
  style; `setlocation {component}` on a box is refused: "<id> is a location box; edit it with
  setprop {aids}"), and `rating {location, temperature}` still
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
6. In the same section set Panel temperature to `45`: it shows `45` at once (no longer faint).
   Pick another Style (add one first in Settings ▸ Standards ▸ Location boxes, for example with a
   continuous line): the outline is redrawn in it; one undo brings the project's style back.

Checked so far: automated headless tests only
(`lanes::lane_072f::renaming_or_captioning_a_shown_arrow_refreshes_its_section` and
`lanes::lane_072f::a_new_location_on_a_shown_box_refreshes_its_section` and
`lanes::lane_072f::a_shown_boxs_panel_temperature_and_style_rows_refresh` in ecad-app;
`lanes::lane_072f` in ecad-cadcraft and ecad-mcp).
Not yet tried by the owner.
