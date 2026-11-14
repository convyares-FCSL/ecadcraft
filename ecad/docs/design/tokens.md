# ECAD design tokens

The one table of ECAD's interface: every colour, size, radius, line weight, type style and icon
name. The source of the values is the UI design spec §4
([`ui-design-spec.md`](../discussions/ui-2026-11/ui-design-spec.md)); the code is the crate
`ecad-tokens` (brief 037, design [`ui-architecture.md`](ui-architecture.md) §3.1), and
`ecad-ui` builds the egui style and the primitives from it. From brief 037 on, nothing in ECAD's
interface is styled locally: ECAD UI code takes colours, sizes and fonts only from `ecad-tokens`
through `ecad-ui` (`cargo xtask layers` checks it).

**These tables are checked.** A test in `ecad-tokens` (`tests/all/doc_sync.rs`) reads every table
under "Tokens" below and compares each row's name and value with the constant of the same name,
both ways: a row without a constant, a constant without a row, or a different value fails. Change
the constant and the row together.

Design specs refer to a token by its name here (`shell.text_2`, `sheet.connection_point`,
`size.control`). CADCraft's own theme (`../crates/ui-egui/src/theme.rs`, `theme::Tokens`) is
still referred to as `cad.<token>` where a spec draws inside CADCraft's chrome.

# Tokens

## Shell colours

The Graphite shell. It has one appearance and does not follow the operating system's light or
dark setting. `ecad_tokens::GRAPHITE`.

| Token | Value | Use |
| --- | --- | --- |
| `shell.bg` | `#131416` | Window ground, the desk behind the sheet |
| `shell.panel` | `#1C1D20` | Panels, top bar, status bar |
| `shell.raised` | `#25272B` | Fields, buttons, group rows, chips |
| `shell.hover` | `#2D3035` | Hovered row or button |
| `shell.active` | `#353941` | Pressed button, active mode, active side panel |
| `shell.line` | `#32353B` | 1 px hairlines between regions and rows |
| `shell.line_strong` | `#464A52` | Popover borders, key caps, badges |
| `shell.text` | `#E8E9EB` | Primary text |
| `shell.text_2` | `#A9ADB5` | Secondary text; values read from the drawing |
| `shell.text_3` | `#8A8F98` | Field labels, placeholders (4.6:1 on `raised`; never text on `hover` or `active`) |
| `shell.accent` | `#2F6BE0` | Primary button, active tool (white text 4.9:1) |
| `shell.accent_text` | `#7FB0FF` | Selected outline, focus ring, links |
| `shell.accent_soft` | `#2F6BE0 at 18 %` | Selected row |
| `shell.ok` | `#55D68B` | Saved, check passed |
| `shell.warning` | `#F2B84B` | Warning |
| `shell.error` | `#FF6B5E` | Error, delete |
| `shell.provisional` | `#FF7AB8` | Provisional and pending values |
| `shell.agent` | `#B9E34A` | Agent presence and agent-made changes |
| `shell.cable` | `#B48CFF` | Cable names in lists |
| `shell.connect` | `#45CFD3` | Links and connection points in lists |

| Token | Value | Use |
| --- | --- | --- |
| `shell.soft` | `18 %` | Opacity of a soft fill: `accent_soft`, and a pill's ground in a status tone |

Every text colour reaches 4.5:1 on the backgrounds it is used on (`Shell::text_pairs`, tested):
`text`, `text_2` and `accent_text` on every ground; `text_3` on `bg`, `panel` and `raised`; the
status and list colours on every ground but `active`; white on `accent`.

## Sheet colours

The paper sheet (`ecad_tokens::sheet::PAPER`, the default) and the dark sheet (`DARK`), the same
meanings in a second set. Ink prints; overlay never does, except the missing-data warning
(`sheet::prints`). Markers are drawn at a fixed screen size and do not scale with zoom. The
canvas switches to these in U4a.

| Token | Paper | Dark | Prints | Marker |
| --- | --- | --- | --- | --- |
| `sheet.sheet` | `#F6F4EF` | `#1B1D21` | no | none |
| `sheet.ink` | `#16191D` | `#E4E6E9` | yes | none |
| `sheet.ink_2` | `#5A616B` | `#9299A3` | yes | none |
| `sheet.grid` | `#D6D2C6` | `#30333A` | no | none |
| `sheet.connection_point` | `#0A8F94` | `#45CFD3` | no | ring, 8 px |
| `sheet.wire_end_attached` | `#17A05B` | `#55D68B` | no | dot, 5 px |
| `sheet.loose_end` | `#7C8591` | `#8B94A1` | no | square, 7 px |
| `sheet.cable` | `#7A3FE0` | `#B48CFF` | yes | slash |
| `sheet.error` | `#D7281E` | `#FF6B5E` | no | disc |
| `sheet.warning` | `#A86400` | `#F2B84B` | no | triangle |
| `sheet.missing_data` | `#A86400` | `#F2B84B` | yes | triangle |
| `sheet.provisional` | `#CF1F7A` | `#FF7AB8` | yes | dotted underline |
| `sheet.terminal_link` | `#0A8F94` | `#45CFD3` | yes | dashed bar |
| `sheet.selection` | `#1F6FEB` | `#5B9BFF` | no | outline |
| `sheet.placement_preview` | `#1F6FEB` | `#5B9BFF` | no | outline |
| `sheet.agent_write` | `#5F8F00` | `#B9E34A` | no | dashed outline |

The selection is filled at 12 % (`sheet.selection_fill`), the placement preview at 8 %
(`sheet.preview_fill`).

| Token | Value |
| --- | --- |
| `sheet.selection_fill` | `12 %` |
| `sheet.preview_fill` | `8 %` |

## Wire colours

Default wire category colours (`ecad_tokens::wire`), changeable in Standards (U4c). A two-colour
wire or core is drawn in both colours: the first solid, the second laid over it as dashes.

| Token | Paper | Dark |
| --- | --- | --- |
| `wire.power` | `#16191D` | `#E4E6E9` |
| `wire.dc_24v` | `#23408F` | `#8FB0FF` |
| `wire.pe` | `#2F8F3A with #E3B800 dashes` | `#6FD67A with #F2D34B dashes` |

| Token | Value |
| --- | --- |
| `wire.dash_on` | `2 mm` |
| `wire.dash_off` | `2 mm` |

## UI type

IBM Plex Sans, px at 100 % UI scale (`ecad_tokens::ui_type`). egui family `ui`, and the first
face of `Proportional`.

| Token | Size / line | Weight | Use |
| --- | --- | --- | --- |
| `ui_type.caption` | `11 / 14 px` | `600` | Section labels, badges, key caps |
| `ui_type.small` | `12 / 16 px` | `400` | Tables at density, status bar, hints |
| `ui_type.body` | `13 / 18 px` | `400` | Everything else |
| `ui_type.body_emphasis` | `13 / 18 px` | `500` | Emphasis in body text |
| `ui_type.title` | `15 / 20 px` | `600` | Panel and card titles |
| `ui_type.object` | `20 / 26 px` | `600` | The selected object's name, screen titles |

## Sheet text

Barlow Semi Condensed, egui family `sheet` (`ecad_tokens::sheet_type`). Sizes are **cap
heights** on paper (owner answer 9); the font size is the cap height divided by 0.70, Barlow Semi
Condensed's cap height in em. Nothing is enlarged for the screen. The sheet switches to these in
U4a. (The spec's table writes 2.15 mm for the terminal label; 1.5 / 0.70 is 2.14 mm.)

| Token | Cap height | Font size | Weight | Use |
| --- | --- | --- | --- | --- |
| `sheet_type.tag` | `2.5 mm` | `3.57 mm` | `600` | Device tag |
| `sheet_type.label` | `1.6 mm` | `2.29 mm` | `400` | Pin label, wire label, reference, continuation |
| `sheet_type.terminal_label` | `1.5 mm` | `2.14 mm` | `400` | Terminal label, description |
| `sheet_type.title_block_label` | `1.4 mm` | `2.00 mm` | `400` | Title block label |
| `sheet_type.zone_label` | `2 mm` | `2.86 mm` | `400` | Zone label |
| `sheet_type.page_title` | `3.5 mm` | `5.00 mm` | `600` | Page title |
| `sheet_type.page_number` | `4.2 mm` | `6.00 mm` | `600` | Page number |

## Spacing and sizes

px at 100 % UI scale (`ecad_tokens::space`, `ecad_tokens::size`).

| Token | Value | Use |
| --- | --- | --- |
| `space.xxs` | `2 px` | Spacing step |
| `space.xs` | `4 px` | Spacing step |
| `space.s` | `6 px` | Spacing step |
| `space.m` | `8 px` | Spacing step |
| `space.l` | `12 px` | Spacing step |
| `space.xl` | `16 px` | Spacing step |
| `space.xxl` | `24 px` | Spacing step |
| `space.panel_padding` | `12 px` | Padding inside a panel |
| `size.control_small` | `24 px` | Small control height |
| `size.control` | `28 px` | Default control height: buttons, fields |
| `size.search_field` | `32 px` | Search field height |
| `size.row_list` | `28 px` | List row |
| `size.row_table` | `30 px` | Table row |
| `size.row_group` | `32 px` | Group row |
| `size.top_bar` | `40 px` | Top bar (UI rev 3) |
| `size.panel_header` | `36 px` | Panel header |
| `size.agent_banner` | `32 px` | Agent banner |
| `size.status_bar` | `32 px` | Bottom bar: status, page well, view (UI rev 3) |
| `size.tool_button` | `44 px` | Tool button height |
| `size.tool_button_w` | `54 px` | Tool button width: an icon over a label (UI rev 3) |
| `size.tool_strip` | `56 px` | Tool strip width; the inspector's collapsed rail |
| `size.tool_icon` | `18 px` | Icon in a tool button |
| `size.tool_label` | `10 px` | Label under a tool button's icon |
| `size.page_well` | `880 px` | The bottom bar's page well |
| `size.icon_button` | `28 px` | Icon button outside the tool strip |
| `size.icon` | `16 px` | Icon grid |

## Radii

px (`ecad_tokens::radius`). A pill is fully rounded: half its height.

| Token | Value | Use |
| --- | --- | --- |
| `radius.chip` | `4 px` | Chips |
| `radius.keycap` | `4 px` | Key caps |
| `radius.badge` | `4 px` | Badges |
| `radius.field` | `5 px` | Fields |
| `radius.button` | `6 px` | Buttons |
| `radius.tool` | `8 px` | Tools |
| `radius.tile` | `8 px` | Tiles |
| `radius.popover` | `10 px` | Popovers |
| `radius.card` | `10 px` | Cards |
| `radius.pill` | `full` | Pills |

## Line weights

UI lines in screen px; canvas lines in mm on paper, scaling with zoom with a floor of 1 px on
screen (`ecad_tokens::line`).

| Token | Value | Use |
| --- | --- | --- |
| `line.hairline` | `1 px` | UI hairline |
| `line.icon_stroke` | `1.5 px` | Icon stroke on a 16 px grid, round caps and joins |
| `line.focus_ring` | `2 px` | Focus ring, `shell.accent_text` |
| `line.screen_floor` | `1 px` | Thinnest canvas line on screen |
| `line.symbol` | `0.35 mm` | Symbol outline |
| `line.control_wire` | `0.4 mm` | Control wire |
| `line.power_wire` | `0.6 mm` | Power wire, PE |
| `line.frame` | `0.5 mm` | Frame |
| `line.rule` | `0.2 mm` | Title block and table rules |
| `line.mechanical_link` | `0.25 mm` | Mechanical link |
| `line.mechanical_link_dash` | `1.2 mm` | Mechanical link dash |
| `line.mechanical_link_gap` | `1 mm` | Mechanical link gap |
| `line.cable_slash` | `0.5 mm` | Cable slash |
| `line.terminal_link` | `0.6 mm` | Terminal link (jumper) bar |
| `line.terminal_link_dash` | `1.8 mm` | Terminal link dash |
| `line.terminal_link_gap` | `1.4 mm` | Terminal link gap |
| `line.selection` | `0.5 mm` | Selection outline |
| `line.agent_write` | `0.6 mm` | Agent's last write outline |

## Wheel and shadow

The wheel widget is U4b (`ecad_tokens::wheel`); the one shadow style is for popovers and the
sheet only (`ecad_tokens::shadow`).

| Token | Value |
| --- | --- |
| `wheel.outer_radius` | `150 px` |
| `wheel.inner_radius` | `54 px` |
| `wheel.slots` | `8` |
| `wheel.slot` | `41 deg` |
| `wheel.gap` | `4 deg` |
| `shadow.offset` | `0 8 px` |
| `shadow.blur` | `24 px` |
| `shadow.color` | `#000000 at 45 %` |

## Icons

Painted in code by `ecad-ui` (`ecad_ui::icons::paint`) from lines, arcs and circles on a 16 px
grid, 1.5 px stroke, round caps and joins. Each is original; none is traced from any product.
Tool icons echo the schematic.

| Token | Label | Drawn as |
| --- | --- | --- |
| `icon.select` | Select | An arrow pointer |
| `icon.wire` | Wire | A line between two end rings |
| `icon.place` | Place | A symbol box with a connection point ring on a lead |
| `icon.cable` | Cable | A slash across three lines |
| `icon.draw` | Draw | An arc over a line |
| `icon.box` | Box | A dashed box |
| `icon.text` | Text | A letter T |
| `icon.workspace_draw` | Draw workspace | A page with a wire on it |
| `icon.workspace_lists` | Lists workspace | Rows with bullets |
| `icon.workspace_library` | Library workspace | Shelves of symbols |
| `icon.workspace_publish` | Publish workspace | A printer with a sheet coming out |
| `icon.search` | Search | A lens |
| `icon.checks` | Checks | A tick in a ring |
| `icon.agent` | Agent | A ring with two eyes |
| `icon.history` | History | A clock arc with hands |
| `icon.pages` | Pages | Stacked sheets |
| `icon.devices` | Devices | A component box with two leads |
| `icon.collapse` | Collapse | A bar and an arrow into it |
| `icon.close` | Close | A cross |
| `icon.chevron_left` | Chevron left | A chevron |
| `icon.chevron_right` | Chevron right | A chevron |
| `icon.chevron_up` | Chevron up | A chevron |
| `icon.chevron_down` | Chevron down | A chevron |
| `icon.delete` | Delete | A bin with a lid (041: an action) |
| `icon.go_to` | Go to | An arrow leaving a box (041: an action) |
| `icon.show_row` | Show row | Rows, the middle one marked (041: an action) |
| `icon.edit` | Edit | A pencil (041: an action) |
| `icon.properties` | Properties | A list in a box (041: an action) |
| `icon.rotate` | Rotate | An open circle with an arrow head (041: an action) |
| `icon.mirror` | Mirror | Two triangles about a dashed axis (041: an action) |
| `icon.duplicate` | Duplicate | Two overlapping boxes (041: an action) |
| `icon.connect` | Connect | A lead into a ring (041: an action) |
| `icon.disconnect` | Disconnect | A lead apart from a ring (041: an action) |
| `icon.number` | Number | A hash (041: an action) |
| `icon.move` | Move | A cross with four arrow heads (041: an action) |
| `icon.paste` | Paste | A sheet on a board with a clip (041: an action) |
| `icon.new_page` | New page | A sheet with a plus (041: an action) |
| `icon.line` | Line | A diagonal with end points (041: an action) |
| `icon.polyline` | Polyline | A bent line (041: an action) |
| `icon.rectangle` | Rectangle | A rectangle (041: an action) |
| `icon.circle` | Circle | A circle (041: an action) |
| `icon.arc` | Arc | A half circle with its centre (041: an action) |
| `icon.terminal` | Terminal | A block with a screw ring between two leads (041: an action) |
| `icon.junction` | Junction | A dot where a line meets a line (041: an action) |
| `icon.cloud` | Revision cloud | Scallops round a box (041: an action) |
| `icon.eye` | Show or hide | An eye (041: an action) |
| `icon.connector` | Connector | A socket's cup over a plug's pin, each on its lead (044: Place ▸ Connector) |
| `icon.new_symbol` | New symbol | A symbol box on two leads, a plus at its top right (048: Library ▸ New symbol) |
| `icon.connection_point` | Connection point | A hollow ring at a lead's end (048: the builder's C tool) |
| `icon.text_slot` | Text slot | A dashed box with an anchor dot (048: the builder's S tool) |
| `icon.origin` | Origin | A cross with a ring at its centre (048: the builder's O tool) |
| `icon.book` | Book | An open book (UI rev 3: the Book panel) |
| `icon.plc` | PLC | A card with a grid of six channels (UI rev 3: the PLC group) |
| `icon.snap` | Snap | A cross hair on a dot (UI rev 3: the Snap toggle) |
| `icon.grid` | Grid | A hash of four lines (UI rev 3: the Grid toggle) |
| `icon.ortho` | Ortho | A right angle with a square corner mark (UI rev 3: the Ortho toggle) |
| `icon.moon` | Dark theme | A crescent (UI rev 3: the light/dark switch) |
| `icon.sun` | Light theme | A ring with eight rays (UI rev 3: the light/dark switch) |
| `icon.help` | Help | A question mark in a ring (UI rev 3: the top bar) |
| `icon.folder` | Page group | A folder with a tab (UI rev 3: a page group in the well) |
| `icon.chevrons_left` | Previous group | Two chevrons (UI rev 3: the page well) |
| `icon.chevrons_right` | Next group | Two chevrons (UI rev 3: the page well) |
| `icon.fit` | Fit | Four corner marks (UI rev 3: the view group) |
| `icon.minus` | Zoom out | A dash (UI rev 3: the view group) |
| `icon.plus` | Zoom in | A plus (UI rev 3: the view group) |
| `icon.pencil` | Draw tools | A pencil (UI rev 3: the Draw group) |
| `icon.pin` | Device tools | A body on a lead (UI rev 3: the Device group) |
| `icon.terminal_pin` | Terminal tools | A lead through a ring with a slash (UI rev 3: the Terminal group) |
| `icon.wire_bend` | Wire tools | A wire bending up from one end ring to another (UI rev 3: the Wire group) |
| `icon.cable_bundle` | Cable tools | Three leads tied by two strokes (UI rev 3: the Cable group) |
| `icon.location_box` | Location tools | A dashed box with a marked corner (UI rev 3: the Location group) |
| `icon.device_grid` | Devices panel | Three squares and a plus (UI rev 3: the Devices panel) |

# Canvas tokens in use until U4a (`ecad.*`)

The rows below are what the canvas and the existing panels use today, in ACI colours on
CADCraft's dark canvas. **Each is retired in U4a**, which moves the canvas to the sheet colours,
markers and sheet text above; until then they stay as they are and are not checked by the doc
test. Where a row has a new token, U4a takes it:

| `ecad.*` (until U4a) | New token |
| --- | --- |
| `ecad.terminal`, `ecad.link` | `sheet.connection_point`, `sheet.terminal_link` |
| `ecad.attached`, `ecad.attach-preview` | `sheet.wire_end_attached` |
| `ecad.loose` | `sheet.loose_end` |
| `ecad.cable`, `ecad.reference` | `sheet.cable` |
| `ecad.refuse-preview`, `ecad.orphan`, `ecad.check-error` | `sheet.error` |
| `ecad.check-warning`, `ecad.flagged` | `sheet.warning` |
| `ecad.provisional` | `sheet.provisional` |
| `ecad.terminal-size`, `ecad.dot-size`, `ecad.loose-size` | `marker` sizes (8, 5, 7 px) |
| `ecad.text-tag`, `ecad.text-label`, `ecad.text-point-label` | `sheet_type.tag`, `sheet_type.label`, `sheet_type.terminal_label` (cap heights) |
| `ecad.tooltip-*`, `ecad.revision-bar`, `ecad.branch-bar`, `ecad.point-bar`, `ecad.history-*` | `shell.*` and the primitives, when U2 to U4 restyle their panels |

## Canvas colours

Colours on the drawing are AutoCAD Color Index (ACI) colours so that they read as CAD and survive
CADCraft's own colour handling. Values are for the dark canvas.

| Token | ACI | RGB | Used for |
| --- | --- | --- | --- |
| `ecad.terminal` | 4 (cyan) | `#00FFFF` | Terminal markers (free or connected) |
| `ecad.attached` | 3 (green) | `#00FF00` | A wire end attached to a terminal |
| `ecad.loose` | 9 (light grey) | `#C0C0C0` | A wire end attached to nothing: a neutral colour, not red, because a dangling end is not necessarily an error (owner answer Q3 to design 008, 2026-11-01) |
| `ecad.attach-preview` | 3 (green) | `#00FF00` | "Will attach" ring and name while a command runs |
| `ecad.preview` | — | CADCraft's rubber-band colour | Rubber band, symbol preview (as CADCraft draws previews) |
| `ecad.cable` | 6 (magenta) | `#FF00FF` | Cables: crossing dots where a slash crosses its members (004), the reserved layer `ECAD-CABLES` (005) and everything computed on it: label box, trunk, spine, bends, tails, core labels, wire numbers and reference flags (009) |
| `ecad.refuse-preview` | 1 (red) | `#FF0000` | "Will be refused" ring and text while a command runs (a slash across a wire already in a cable, 004) |
| `ecad.reference` | 6 (magenta) | `#FF00FF` | A linked cable reference: its ring and its derived `/page.column` text (010); same colour as `ecad.cable` |
| `ecad.agent` (035) | `cad.accent` | — | Agent banner's left bar while connected (the styled banner, UI shell) |
| `ecad.agent-write` (035) | 3 (green) | `#00FF00` | Agent banner's left bar for 1 s after an accepted agent write; same as `ecad.attached` |
| `ecad.agent-bar` (035) | `cad.chrome_dark` | — | The agent banner strip: fill, 3 px left bar, 24 px high (the revision and branch bar pattern) |
| `ecad.orphan` | 1 (red) | `#FF0000` | A cable reference with no other end: ring and `/?` text (010), the concept spec's red orphan marker |
| `ecad.wire-label` | 7 (white), ByLayer on `ECAD-WIRE-LABELS` | `#FFFFFF` | Conductor labels: wire number, cross-section, colour (022) |
| `ecad.check-error` | 1 (red) | `#FF0000` | Checks error glyph and go-to ring (022); same as `ecad.orphan` |
| `ecad.check-warning` | 2 (yellow) | `#FFFF00` | Checks warning glyph and go-to ring (022) |
| `ecad.continuation` | ByLayer | — | Continuation arrow (022), drawn on its symbol's layer |
| `ecad.provisional` | 30 (orange) | `#FF7F00` | Provisional tags on the sheet, provisional wire labels (`24V-?`), provisional text in lists, fields and the Renumber preview (026) |
| `ecad.link` | 4 (cyan) | `#00FFFF` | A terminal link (jumper) line through its terminals, dashed 2 mm / 1 mm, `ecad.hairline`; lanes 1.5 mm apart; continuation stub 4 mm (030 §5.3); derived, never stored |
| `ecad.fixed-mark` | `cad.text_dim`, small text | — | The `F` after a fixed tag in the Devices tab and the Renumber preview (026); not drawn on the sheet |

Drawn geometry (rectangles, wires, symbol graphics, cable slashes) keeps its own layer colour: ECAD
does not recolour stored objects. Computed drawing that is not stored (the fan view of a cable) is
drawn in `ecad.cable` so it reads as a view, not as editable geometry.

## Sizes and line weights

Glyph sizes are in **screen pixels** at 100 % UI scale and do not change with zoom (a glyph is the
same size at every zoom). They scale with the UI scale like CADCraft's own snap markers.

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.hairline` | 1 px | Terminal circle, loose-end square, preview lines |
| `ecad.bold` | 2 px | Attach-preview ring (matches CADCraft's snap marker stroke) |
| `ecad.terminal-size` | 8 px diameter | Terminal marker circle |
| `ecad.dot-size` | 4 px diameter, filled | Attached-end dot |
| `ecad.loose-size` | 6 px square | Loose-end square |
| `ecad.ring-size` | 16 px diameter | Attach-preview ring; also the refuse-preview ring |
| `ecad.crossing-size` | 5 px diameter, filled | Crossing dot of a cable slash on a member wire |
| `ecad.ref-size` | 12 px diameter | Ring on a cable reference point (010), stroke `ecad.bold` |
| `ecad.preview-ring` | 20 px diameter | Ring around the target reference in the read-only preview (016), stroke `ecad.bold`, colour `ecad.reference` |
| `ecad.aperture` | CADCraft `APERTURE` setting (default 10 px) | How close the cursor must be for an ECAD capture |

## Drawing-unit sizes

Sizes of computed drawing (009): in **drawing units**, so they scale with zoom and plot as drawn,
unlike the screen-pixel glyphs above. Fixed values for now; per-project drawing standards are later
work. Brief 010's values are millimetres: in an inch project they are divided by 25.4.

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.label-height` | 2.5 drawing units | Core labels, wire numbers, label box lines, reference text |
| `ecad.label-gap` | 1.0 drawing unit (0.4 × label height) | Gap between a bend and the first label, between labels, after the last |
| `ecad.tail-min` | 20 drawing units | Shortest tail of a core in fan view |
| `ecad.bend-radius` | 2.5 drawing units, capped at half the core pitch | Bend where a core leaves the spine |
| `ecad.spine-lead` | 5 drawing units | Not used since the trunk joins the middle of the spine (owner request, 2026-11-07) |
| `ecad.box-line-pitch` | 1.5 × label height | Line spacing in the cable label box |
| `ecad.box-pad` | 0.5 × label height | Padding in the label box and the reference flag |
| `ecad.ref-text-height` | 2.5 mm | Derived text of a cable reference (010) |
| `ecad.ref-stub-spacing` | 5 mm | Spacing between continued wire stubs, and the gap between a reference point and its text (010) |
| `ecad.column-width` | 40 mm | **Retired by 019**: was the fixed column grid (from x = 0) of reference texts; columns now come from the page's sheet frame (019 §3.3) |
| `ecad.preview-span` | 160 mm | Width of drawing the read-only preview shows when opened on a reference (016) |
| `ecad.tag` | ACI 7 (ByLayer on `ECAD-TAGS`) | Device tag texts and connection point labels (018) |
| `ecad.tag-height` | 3.5 mm | Device tag text beside each symbol (018) |
| `ecad.term-label-height` | 2.0 mm | Connection point labels of a symbol (018) |
| `ecad.tag-gap` | 2.0 mm | Gap between a symbol's box and its tag (018) |
| `ecad.symbol-grid` | 5 mm | Connection point grid of the starter symbols; snap and grid spacing of ECAD pages (018) |
| `ecad.junction-size` | 0.5 mm radius, lineweight 0.50 mm | Junction dot (022) |

## Sheet frame and title block (019)

Drawn on the reserved layer `ECAD-FRAME`; sizes in millimetres (divided by 25.4 in an inch project).

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.frame` | ACI 7 (ByLayer on `ECAD-FRAME`) | Frame, grid labels, ticks, title-block lines and values |
| `ecad.frame-label` | ACI 8 | Title-block box labels |
| `ecad.sheet-edge` | ACI 8 | Paper edge line |
| `ecad.frame-weight` | 0.50 mm (lineweight code 50) | Frame rectangle and the title block's top line |
| `ecad.grid-label-height` | 3.0 mm (column labels), 3.5 mm (row labels) | Grid labels |
| `ecad.tb-label-height` | 1.8 mm | Title-block box labels |
| `ecad.tb-value-height` | 2.5 mm (3.5 project and page title, 5.0 rev, 7.0 page number) | Title-block values |
| `ecad.revision-bar` | `cad.chrome_dark` fill, 3 px `cad.accent` left bar | Read-only revision bar in the Project panel |

## Line weights of computed drawing

Entity lineweights (shown on screen when CADCraft's `LWDISPLAY` is on, which the trial turns on;
always carried to plots and exports). Thick is the cable section, thin the wire section (009).

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.cable-weight` | 0.70 mm (lineweight code 70) | Trunk and spine |
| `ecad.wire-weight` | 0.25 mm (lineweight code 25) | Bends, tails, label box frame, reference flag |

## Panels and text

| Token | Value |
| --- | --- |
| `ecad.tooltip-bg` | `cad.panel` |
| `ecad.tooltip-border` | `cad.border`, 1 px, square corners |
| `ecad.tooltip-text` | `cad.text`, monospace, CADCraft's small text size |
| `ecad.tooltip-dim` | `cad.text_dim` (secondary lines: IDs, "not connected") |
| `ecad.preview-text` | `ecad.attach-preview`, monospace, 12 px screen height |

## Light theme

The ECAD trial is used with CADCraft's dark theme. On the light theme, ACI cyan and green have poor
contrast; until a spec says otherwise ECAD keeps the same ACI colours (as CADCraft does for drawing
colours) and this is a known limitation.


## History graph (020, `design/020.md` §11; in use since 020a)

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.history-lane` | `cad.text_dim`, 1 px | History graph lines |
| `ecad.history-current` | `cad.accent`, 2 px | The current branch's lane and its ancestry |
| `ecad.history-revision` | `cad.accent`, filled diamond 9 px | Issued revision nodes |
| `ecad.history-version` | `cad.text`, hollow circle 8 px, 1 px stroke | Version nodes |
| `ecad.history-auto` | `cad.text_faint`, filled dot 3 px | Automatic snapshots, when shown |
| `ecad.branch-bar` | `cad.chrome_dark` fill, 3 px `cad.accent` left bar | Branch bar in the Project panel |
| `ecad.point-bar` | same as `ecad.revision-bar` | Read-only point bar (019's revision bar, generalised) |

## Outputs (024, `design/024.md` §12)

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.location-box` | ACI 7, ByLayer on `ECAD-LOCATIONS`, linetype `DASHED`, lineweight `ecad.wire-weight` | Location boxes |
| `ecad.location-label-height` | 3.5 mm | `+…` label of a box |
| `ecad.generated` | ACI 7, layer `ECAD-GENERATED` (locked) | Tables and text of generated pages |
| `ecad.table-row` / `ecad.table-header` | 5 mm / 7 mm | Generated table rows |
| `ecad.flagged` | `ecad.check-warning` | `?` values on screen and in Reports |

## Print and project (033, `design/033.md` §13)

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.flagged-print` | ACI 7 | A missing value's `?` and its box when printed with screen colours |
| `ecad.missing-box` | `ecad.wire-weight`, inset 0.5 mm, colour of its `?` | The box round a missing value in a generated table |

## Text styles (031, `design/031.md` §3 and §9)

Text sizes and weights come from the project's style table through `text_style::text_style(doc,
kind)`; the values below are its defaults (millimetres, divided by 25.4 in an inch project). They
replace `ecad.tag-height`, `ecad.term-label-height`, `ecad.label-height`, `ecad.ref-text-height`
and 024's location label height when 031 lands.

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.text-tag` | 2.5 mm, bold | Device tags |
| `ecad.text-point-label` | 1.5 mm | Connection point labels |
| `ecad.text-label` | 1.6 mm | Wire labels, cable text, location labels, descriptions, part numbers |
| `ecad.text-reference` | 2.0 mm | Cross-reference texts and continuation names |
| `ecad.tag-gap` (031) | 0.6 × tag height | Gap between a symbol's box and its tag block |
| `ecad.hidden-text` | ACI 8, on `ECAD-HIDDEN-TEXT` | Hidden derived texts while Show Hidden Text is on; never printed |
| `ecad.stroke-bold` | lineweight 0.35 mm | Bold text drawn in the stroke-font fallback |

## Multi-level terminals (036, `design/036.md` §7.5)

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.level-box` | `cad.text_dim`, 1 px on screen; 0.25 mm on generated pages | The position box spanning a position's level rows in the terminal plan |
| `ecad.field-editable` | `cad.border` 1 px, square corners, CADCraft's text-edit fill | A plan cell that edits in place; derived cells have no border |
