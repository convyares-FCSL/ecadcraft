# Design tokens

The one table of ECAD's interface: every colour, size, radius, line weight, type style and icon name. The code
is the `tokens` module of `ecad-widgets`, which also builds the egui style and the components from it. Nothing
in ECAD's interface is styled locally: ECAD UI code takes colours, sizes and fonts only from
`ecad_widgets::tokens` (`cargo xtask layers` checks it).

**These tables are checked.** A test in `ecad-widgets` (`tests/all/tokens/doc_sync.rs`) reads every table under
"Tokens" below (the screen's look) and compares each row's name and value with the constant of the same name,
both ways: a row without a constant, a constant without a row, or a different value fails. A test in
`ecad-model` (`tests/all/palette/doc_sync.rs`) does the same for the tables under "Drawing style" (the canvas
palette). Change the constant and the row together. The canvas tokens (`ecad.*`) after them are not checked.

Design notes refer to a token by its name here (`shell.text_2`, `sheet.connection_point`, `size.control`).
CADCraft's own theme (`theme::Tokens` in CADCraft's `ui-egui` crate) is referred to as `cad.<token>` where a
note draws inside CADCraft's chrome.

# Tokens

## Shell colours

The Graphite shell, `ecad_widgets::tokens::GRAPHITE`, and the light shell, `ecad_widgets::tokens::LIGHT`.
The person picks one with the top bar's light/dark switch; neither follows the operating system's setting.
Code reads the one in use through `ecad_widgets::SHELL`.

| Token | Graphite | Light | Use |
| --- | --- | --- | --- |
| `shell.bg` | `#131416` | `#E6E8EB` | Window ground, the desk behind the sheet |
| `shell.panel` | `#1C1D20` | `#F7F8F9` | Panels, top bar, status bar |
| `shell.raised` | `#25272B` | `#FFFFFF` | Fields, buttons, group rows, chips |
| `shell.hover` | `#2D3035` | `#EDEFF2` | Hovered row or button |
| `shell.active` | `#353941` | `#E1E5EB` | Pressed button, active mode, active side panel |
| `shell.line` | `#32353B` | `#D5D9DF` | 1 px hairlines between regions and rows |
| `shell.line_strong` | `#464A52` | `#BCC3CC` | Popover borders, key caps, badges |
| `shell.text` | `#E8E9EB` | `#16191D` | Primary text |
| `shell.text_2` | `#A9ADB5` | `#474D56` | Secondary text; values read from the drawing |
| `shell.text_3` | `#8A8F98` | `#5F6670` | Field labels, placeholders (4.6:1 on `raised`; never text on `hover` or `active`) |
| `shell.accent` | `#2F6BE0` | `#2F6BE0` | Primary button, active tool (white text 4.9:1) |
| `shell.accent_text` | `#7FB0FF` | `#1F5FCC` | Selected outline, focus ring, links |
| `shell.accent_soft` | `#2F6BE0 at 18 %` | `#2F6BE0 at 12 %` | Selected row |
| `shell.ok` | `#55D68B` | `#116E3E` | Saved, check passed |
| `shell.warning` | `#F2B84B` | `#8A5200` | Warning |
| `shell.error` | `#FF6B5E` | `#C4231A` | Error, delete |
| `shell.provisional` | `#FF7AB8` | `#B0186A` | Provisional and pending values |
| `shell.agent` | `#B9E34A` | `#4A7200` | Agent presence and agent-made changes |
| `shell.cable` | `#B48CFF` | `#6A35C8` | Cable names in lists |
| `shell.connect` | `#45CFD3` | `#066A6E` | Links and connection points in lists |
| `shell.on_accent` | `#FFFFFF` | `#FFFFFF` | Ink on the accent and on a filled status ground (primary button label, a ticked box) |
| `shell.ai_ink` | `#14180A` | `#FFFFFF` | Ink on the agent's colour: the `AI` chip's letters |

| Token | Value | Use |
| --- | --- | --- |
| `shell.soft` | `18 %` | Opacity of a soft fill: `accent_soft`, and a pill's ground in a status tone |
| `shell.light_soft` | `12 %` | The light shell's `accent_soft` (18 % reads too strong on a light ground) |

Every text colour reaches 4.5:1 on the backgrounds it is used on (`Shell::text_pairs`, tested):
`text`, `text_2` and `accent_text` on every ground; `text_3` on `bg`, `panel` and `raised`; the
status and list colours on every ground but `active`; `on_accent` on `accent`; `ai_ink` on `agent`. The same holds for the
light shell (tested too); its status and list colours are darker than Graphite's for that reason.

## UI type

IBM Plex Sans, px at 100 % UI scale (`ecad_widgets::tokens::ui_type`). egui family `ui`, and the first
face of `Proportional`.

| Token | Size / line | Weight | Use |
| --- | --- | --- | --- |
| `ui_type.caption` | `11 / 14 px` | `600` | Section labels, badges, key caps |
| `ui_type.small` | `12 / 16 px` | `400` | Tables at density, status bar, hints |
| `ui_type.body` | `13 / 18 px` | `400` | Everything else |
| `ui_type.body_emphasis` | `13 / 18 px` | `500` | Emphasis in body text |
| `ui_type.title` | `15 / 20 px` | `600` | Panel and card titles |
| `ui_type.object` | `20 / 26 px` | `600` | The selected object's name, screen titles |
| `ui_type.micro` | `9 / 12 px` | `400` | Point keys and slot labels on symbol drawings in the Library |
| `ui_type.lead` | `14 / 20 px` | `400` | An empty screen's hint |

## Spacing and sizes

px at 100 % UI scale (`ecad_widgets::tokens::space`, `ecad_widgets::tokens::size`).

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
| `size.top_bar` | `40 px` | Top bar |
| `size.panel_header` | `36 px` | Panel header |
| `size.agent_banner` | `32 px` | Agent banner |
| `size.status_bar` | `32 px` | Bottom bar: status, page well, view |
| `size.tool_button` | `44 px` | Tool button height |
| `size.tool_button_w` | `54 px` | Tool button width: an icon over a label |
| `size.tool_strip` | `56 px` | Tool strip width; the inspector's collapsed rail |
| `size.tool_icon` | `18 px` | Icon in a tool button |
| `size.tool_label` | `10 px` | Label under a tool button's icon |
| `size.page_well` | `880 px` | The bottom bar's page well |
| `size.icon_button` | `28 px` | Icon button outside the tool strip |
| `size.icon` | `16 px` | Icon grid |
| `size.label_column` | `96 px` | The label column of a panel's value rows (inspector, Project panel) |
| `size.chevron` | `16 px` | A section header's fold chevron |
| `size.dialog_s` | `400 px` | A small dialog's content width: a question, a name to type |
| `size.dialog_m` | `560 px` | A dialog's usual content width: a short form, a list to choose from |

## Radii

px (`ecad_widgets::tokens::radius`). A pill is fully rounded: half its height.

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
| `radius.row` | `4 px` | A row's hover or selection ground, a small button's hover |
| `radius.check` | `4 px` | Check boxes |
| `radius.segment` | `5 px` | The selected segment in a segmented control (button less its 1 px inset) |
| `radius.inset_card` | `6 px` | A card inside a panel section (inspector part cards) |
| `radius.pill` | `full` | Pills |

## Line weights

UI lines in screen px (`ecad_widgets::tokens::line`). Canvas lines are in mm on paper: they are part of
the drawing style ("Line weights on the page" below), and `line.screen_floor` is the thinnest a
canvas line is drawn on screen.

| Token | Value | Use |
| --- | --- | --- |
| `line.hairline` | `1 px` | UI hairline |
| `line.icon_stroke` | `1.5 px` | Icon stroke on a 16 px grid, round caps and joins |
| `line.focus_ring` | `2 px` | Focus ring, `shell.accent_text` |
| `line.screen_floor` | `1 px` | Thinnest canvas line on screen |

## Wheel and shadow

The wheel widget's sizes (`ecad_widgets::tokens::wheel`); the one shadow style is for popovers and the
sheet only (`ecad_widgets::tokens::shadow`).

| Token | Value |
| --- | --- |
| `wheel.outer_radius` | `94 px` |
| `wheel.inner_radius` | `32 px` |
| `wheel.slots` | `8` |
| `wheel.slot` | `43 deg` |
| `wheel.gap` | `2 deg` |
| `shadow.offset` | `0 8 px` |
| `shadow.blur` | `24 px` |
| `shadow.color` | `#000000 at 45 %` |

## Icons

Painted in code by `ecad-widgets` (`ecad_widgets::icons::paint`) from lines, arcs and circles on a 16 px
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
| `icon.delete` | Delete | A bin with a lid (an action) |
| `icon.go_to` | Go to | An arrow leaving a box (an action) |
| `icon.show_row` | Show row | Rows, the middle one marked (an action) |
| `icon.edit` | Edit | A pencil (an action) |
| `icon.properties` | Properties | A list in a box (an action) |
| `icon.rotate` | Rotate | An open circle with an arrow head (an action) |
| `icon.mirror` | Mirror | Two triangles about a dashed axis (an action) |
| `icon.duplicate` | Duplicate | Two overlapping boxes (an action) |
| `icon.connect` | Connect | A lead into a ring (an action) |
| `icon.disconnect` | Disconnect | A lead apart from a ring (an action) |
| `icon.number` | Number | A hash (an action) |
| `icon.move` | Move | A cross with four arrow heads (an action) |
| `icon.paste` | Paste | A sheet on a board with a clip (an action) |
| `icon.new_page` | New page | A sheet with a plus (an action) |
| `icon.line` | Line | A diagonal with end points (an action) |
| `icon.polyline` | Polyline | A bent line (an action) |
| `icon.rectangle` | Rectangle | A rectangle (an action) |
| `icon.circle` | Circle | A circle (an action) |
| `icon.arc` | Arc | A half circle with its centre (an action) |
| `icon.terminal_block` | Terminal | A block with a screw ring between two leads (an action) |
| `icon.junction` | Junction | A dot where a line meets a line (an action) |
| `icon.cloud` | Revision cloud | Scallops round a box (an action) |
| `icon.eye` | Show or hide | An eye (an action) |
| `icon.connector` | Connector | A socket's cup over a plug's pin, each on its lead (Place ▸ Connector) |
| `icon.new_symbol` | New symbol | A symbol box on two leads, a plus at its top right (Library ▸ New symbol) |
| `icon.connection_point` | Connection point | A hollow ring at a lead's end (the builder's C tool) |
| `icon.text_slot` | Text slot | A dashed box with an anchor dot (the builder's S tool) |
| `icon.origin` | Origin | A cross with a ring at its centre (the builder's O tool) |
| `icon.book` | Book | An open book (the Book panel) |
| `icon.plc` | PLC | A card with a grid of six channels (the PLC group) |
| `icon.snap` | Snap | A cross hair on a dot (the Snap toggle) |
| `icon.grid` | Grid | A hash of four lines (the Grid toggle) |
| `icon.ortho` | Ortho | A right angle with a square corner mark (the Ortho toggle) |
| `icon.moon` | Dark theme | A crescent (the light/dark switch) |
| `icon.sun` | Light theme | A ring with eight rays (the light/dark switch) |
| `icon.help` | Help | A question mark in a ring (the top bar) |
| `icon.folder` | Page group | A folder with a tab (a page group in the well) |
| `icon.chevrons_left` | Previous group | Two chevrons (the page well) |
| `icon.chevrons_right` | Next group | Two chevrons (the page well) |
| `icon.fit` | Fit | Four corner marks (the view group) |
| `icon.minus` | Zoom out | A dash (the view group) |
| `icon.plus` | Zoom in | A plus (the view group) |
| `icon.pencil` | Draw tools | A pencil (the Draw group) |
| `icon.pin` | Device tools | A body on a lead (the Device group) |
| `icon.terminal_block_pin` | Terminal tools | A lead through a ring with a slash (the Terminal group) |
| `icon.wire_bend` | Wire tools | A wire bending up from one end ring to another (the Wire group) |
| `icon.cable_bundle` | Cable tools | Three leads tied by two strokes (the Cable group) |
| `icon.location_box` | Location tools | A dashed box with a marked corner (the Location group) |
| `icon.device_grid` | Devices panel | Three squares and a plus (the Devices panel) |
| `icon.share` | Share | An arrow rising out of a tray (the top bar) |

# Drawing style

How a page is drawn and printed: sheet and canvas colours, wire colours, markers
and line weights on the page. The code is the canvas palette, `ecad_model::palette` (Core), which the
engine, `ecad-widgets` and the app all read; a test in `ecad-model` (`tests/all/palette/doc_sync.rs`)
checks these tables the same way as the screen tokens above. Text heights and fonts are the text
styles, not a table here.

## Sheet colours

The paper sheet (`ecad_model::palette::sheet::PAPER`, the default) and the dark sheet (`DARK`),
the same meanings in a second set. Ink prints; overlay never does, except the missing-data warning
(`sheet::prints`). Markers are drawn at a fixed screen size and do not scale with zoom.

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
| `sheet.jumper_bar` | `#0A8F94` | `#45CFD3` | yes | dashed bar |
| `sheet.selection` | `#1F6FEB` | `#5B9BFF` | no | outline |
| `sheet.placement_preview` | `#1F6FEB` | `#5B9BFF` | no | outline |
| `sheet.agent_write` | `#5F8F00` | `#B9E34A` | no | dashed outline |

The selection is filled at 12 % (`sheet.selection_fill`), the placement preview at 8 %
(`sheet.preview_fill`). Page thumbnails and the page menu's previews are drawn in neutral greys
on either sheet (`sheet.thumbnail_paper`, `sheet.preview_paper`, `sheet.thumbnail_ink`).

| Token | Value |
| --- | --- |
| `sheet.selection_fill` | `12 %` |
| `sheet.preview_fill` | `8 %` |
| `sheet.thumbnail_paper` | `#E8E8E8` |
| `sheet.preview_paper` | `#ECECEC` |
| `sheet.thumbnail_ink` | `#464646` |

## Wire colours

Default wire type colours (`ecad_model::palette::wire`), changeable in Standards. A
two-colour wire or core is drawn in both colours: the first solid, the second laid over it as dashes.

| Token | Paper | Dark |
| --- | --- | --- |
| `wire.power` | `#16191D` | `#E4E6E9` |
| `wire.dc_24v` | `#23408F` | `#8FB0FF` |
| `wire.pe` | `#2F8F3A with #E3B800 dashes` | `#6FD67A with #F2D34B dashes` |

| Token | Value |
| --- | --- |
| `wire.dash_on` | `2 mm` |
| `wire.dash_off` | `2 mm` |

## Line weights on the page

Canvas lines in mm on paper, scaling with zoom with a floor of `line.screen_floor` on screen
(`ecad_model::palette::line`).

| Token | Value | Use |
| --- | --- | --- |
| `line.symbol` | `0.35 mm` | Symbol outline |
| `line.control_wire` | `0.4 mm` | Control wire |
| `line.power_wire` | `0.6 mm` | Power wire, PE |
| `line.frame` | `0.5 mm` | Frame |
| `line.rule` | `0.2 mm` | Title block and table rules |
| `line.mechanical_link` | `0.25 mm` | Mechanical link |
| `line.mechanical_link_dash` | `1.2 mm` | Mechanical link dash |
| `line.mechanical_link_gap` | `1 mm` | Mechanical link gap |
| `line.cable_slash` | `0.5 mm` | Cable slash |
| `line.jumper_bar` | `0.6 mm` | Jumper bar |
| `line.jumper_bar_dash` | `1.8 mm` | Jumper bar dash |
| `line.jumper_bar_gap` | `1.4 mm` | Jumper bar gap |
| `line.selection` | `0.5 mm` | Selection outline |
| `line.agent_write` | `0.6 mm` | Agent's last write outline |

# Canvas tokens (`ecad.*`)

What ECAD draws through CADCraft's entities (computed drawing, overlays, generated pages) uses AutoCAD Color
Index (ACI) colours and the sizes below. Code names each value by its token in a doc comment next to the
constant. These rows are not checked by a test; the sheet colours, markers and line weights under "Drawing
style" are the checked set. A row stays here only while code names its token.

## Canvas colours

Colours on the drawing are ACI colours so that they read as CAD and survive CADCraft's own colour handling.
Values are for the dark canvas.

| Token | ACI | RGB | Used for |
| --- | --- | --- | --- |
| `ecad.terminal` | 4 (cyan) | `#00FFFF` | Connection point markers on CADCraft's canvas (kept: the `ecad-widgets` doc-sync test names this row) |
| `ecad.attach-preview` | 3 (green) | `#00FF00` | "Will attach" ring and name while a command runs; the preview text that says what will happen |
| `ecad.cable` | 6 (magenta) | `#FF00FF` | Cables: crossing dots where a slash crosses its members, the reserved layer `ECAD-CABLES` and everything computed on it: label box, trunk, spine, bends, tails, core labels, wire numbers and reference flags |
| `ecad.refuse-preview` | 1 (red) | `#FF0000` | "Will be refused" ring and text while a command runs (a slash across a wire already in a cable) |
| `ecad.reference` | 6 (magenta) | `#FF00FF` | A linked cable reference: its ring and its derived text; the colour of `ECAD-CABLES`, so the text is ByLayer |
| `ecad.orphan` | 1 (red) | `#FF0000` | A cable reference with no other end (ring and `?` text), contact mirror rows beyond the offer, a net in a wire-number error |
| `ecad.wire-label` | 7 (white), ByLayer on the wire label layer | `#FFFFFF` | Wire labels: wire number, cross-section, colour |
| `ecad.check-warning` | 2 (yellow) | `#FFFF00` | Checks warning ring; the ring on a symbol inside a location box that holds another value |
| `ecad.provisional` | 30 (orange) | `#FF7F00` | Provisional tags on the sheet, provisional wire labels (`24V-?`), provisional text in lists, fields and the Renumber preview |
| `ecad.references` | 7, ByLayer on `ECAD-TAGS` | `#FFFFFF` | Device references: contact texts, contact mirror table labels and places. Its text is reference text (the `Reference` text style), never a size of its own |
| `ecad.xref-spare` | 8 | `#808080` | Spare rows of a contact mirror table |
| `ecad.title-red` | 1 (red) | `#FF0000` | A title block field set to Red (house style: the status) |

Drawn geometry (rectangles, wires, symbol graphics, cable slashes) keeps its own layer colour: ECAD does not
recolour stored objects. Computed drawing that is not stored (the fan view of a cable) is drawn in `ecad.cable`
so it reads as a view, not as editable geometry.

## Sizes and line weights

Glyph sizes are in **screen pixels** at 100 % UI scale and do not change with zoom (a glyph is the same size at
every zoom). They scale with the UI scale like CADCraft's own snap markers.

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.bold` | 2 px | Stroke of the attach-preview ring and of a cable reference ring |
| `ecad.crossing-size` | 5 px diameter, filled | Crossing dot of a cable slash on a member wire |
| `ecad.ref-size` | 12 px diameter | Ring on a cable reference point, stroke `ecad.bold` |
| `ecad.preview-ring` | 20 px diameter | Ring around the target reference in the read-only preview, stroke `ecad.bold`, colour `ecad.reference` |

## Drawing-unit sizes

Sizes of computed drawing: in **drawing units**, so they scale with zoom and plot as drawn, unlike the
screen-pixel glyphs above. Values given in millimetres are divided by 25.4 in an inch project.

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.label-gap` | 1.0 drawing unit (0.4 × label height) | Gap between a bend and the first label, between labels, after the last |
| `ecad.tail-min` | 20 drawing units | Shortest tail of a core in fan view |
| `ecad.bend-radius` | 2.5 drawing units, capped at half the core pitch | Bend where a core leaves the spine |
| `ecad.box-line-pitch` | 1.5 × label height | Line spacing in the cable label box |
| `ecad.box-pad` | 0.5 × label height | Padding in the label box and the reference flag |
| `ecad.ref-stub-spacing` | 5 mm | Spacing between continued wire stubs, and the gap between a reference point and its text |
| `ecad.preview-span` | 160 mm | Width of drawing the read-only preview shows when opened on a reference |
| `ecad.symbol-grid` | 5 mm | Connection point grid of the built-in symbols; snap and grid spacing of ECAD pages |
| `ecad.mirror-row` | 4.0 mm | Row pitch of a contact mirror table (2 mm more before the first auxiliary row) |

## Sheet frame and title block

Drawn on the reserved layer `ECAD-FRAME`; sizes in millimetres.

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.frame` | ACI 7 (ByLayer on `ECAD-FRAME`) | Frame, grid labels, ticks, title-block lines and values |
| `ecad.frame-label` | ACI 8 | Title-block box labels |
| `ecad.sheet-edge` | ACI 8 | Paper edge line |
| `ecad.revision-bar` | `cad.chrome_dark` fill, 3 px `cad.accent` left bar | Read-only bar shown while an issued revision is open |

## Line weights of computed drawing

Entity lineweights, always carried to plots and exports. Thick is the cable section, thin the wire section.

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.cable-weight` | 0.70 mm (lineweight code 70) | Trunk and spine |
| `ecad.wire-weight` | 0.25 mm (lineweight code 25) | Bends, tails, label box frame, reference flag, frame lines |

## Panels and text

| Token | Value |
| --- | --- |
| `ecad.preview-text` | `ecad.attach-preview`, monospace, 12 px screen height |

## Light theme

The shell has a light set (Shell colours above). The sheet's colour is chosen apart from it (Paper, Follow,
Dark: the bottom bar's sheet colour menu), so the canvas tokens are unchanged. CADCraft's own ACI colours are not
themed: on a light interface ACI cyan and green keep their poor contrast, a known limitation.

## History graph

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.history-lane` | `cad.text_dim`, 1 px | History graph lines (kept: the `ecad-widgets` doc-sync test names this row) |
| `ecad.history-current` | `cad.accent`, 2 px | The current branch's lane and its ancestry |

## Location and function boxes

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.location-box` | ACI 7, ByLayer on `ECAD-LOCATIONS`, linetype `DASHED`, lineweight `ecad.wire-weight` | Location boxes |
| `ecad.function-box.linetype` | `DASHDOT`, our own definition | Outline of a box that carries a function (a function box, or a box with both a function and a location) |

## Reports

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.generated` | ACI 7, layer `ECAD-GENERATED` (locked) | Tables and text of generated pages |
| `ecad.table-row` / `ecad.table-header` | 5 mm / 7 mm | Generated table rows (the `[layout]` values in `pages.toml`) |
| `ecad.flagged` | `ecad.check-warning` | `?` values on screen and in Reports |

## Print

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.flagged-print` | ACI 7 | A missing value's `?` and its box when printed with screen colours |
| `ecad.missing-box` | `ecad.wire-weight`, inset 0.5 mm, colour of its `?` | The box round a missing value in a generated table |

## Text styles

Text sizes and weights come from the project's text styles.

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.text-tag` | 2.5 mm, bold | Device tags (kept: the `ecad-widgets` doc-sync test names this row) |
| `ecad.hidden-text` | ACI 8, on `ECAD-HIDDEN-TEXT` | Hidden derived texts while Show Hidden Text is on; never printed |

## Multi-level terminal blocks

| Token | Value | Used for |
| --- | --- | --- |
| `ecad.dot-size` | 0.5 mm radius | A connection dot on a terminal strip page |
| `ecad.level-box` | `cad.text_dim`, 1 px on screen; 0.25 mm on generated pages | The position box spanning a position's level rows in the terminal plan |

## Wire codes

`ecad.wire_colour.<code>`, IEC 60757 codes, paper and dark (`ecad_electrical::cables::words::CODE_COLOURS`):

| Code | Paper | Dark |
| --- | --- | --- |
| BK | ink `#16191D` | ink `#E4E6E9` |
| BN | `#8A4B1F` | `#C9895A` |
| RD | `#C62828` | `#FF7A70` |
| OG | `#D96800` | `#FFA34D` |
| YE | `#E3B800` | `#F2D34B` |
| GN | `#2F8F3A` | `#6FD67A` |
| BU | `#23408F` | `#8FB0FF` |
| VT | `#7B3FA0` | `#C392F0` |
| GY | `#7C8591` | `#A3ABB6` |
| WH | `#B9B4A6` | `#F2F3F5` |
| PK | `#D9528F` | `#FF9CC8` |
| GD | `#A8851C` | `#E3C463` |
| TQ | `#14948F` | `#54D3CC` |
| SR | `#98A0A8` | `#C9CED4` |

GN, YE and BU equal the PE and 24 V wire type colours.

## Terminal strip pages

All ink (ACI 7): a terminal strip page prints in mono like every generated page.

| Token | Value | Use |
| --- | --- | --- |
| `ecad.strip.pe-hatch` | ink, 0.13 mm, 45°, 1 mm pitch | PE levels |
| `ecad.strip.accessory` | ink, solid fill | End stops, end plates, partitions |
| `ecad.strip.lane-gap` | 2 mm jumper bars, 2.5 mm cable bars | Lane spacing |

Also used there: `ecad.dot-size`, `ecad.level-box`, `ecad.cable-weight`, `ecad.missing-box`, `ecad.flagged-print`.

## Symbol builder

| Token | Value | Use |
| --- | --- | --- |
| `ecad.builder-grid-minor` | 2.5 mm, `sheet.grid` dot 1 px | Builder grid |
| `ecad.builder-grid-major` | 5 mm, `sheet.grid` dot 2 px | Point grid |
| `ecad.builder-origin` | `sheet.ink_2`, 4 mm arms, 0.18 mm | Origin cross |
| `ecad.slot-anchor` | `sheet.placement_preview`, 2 px dot | Text slot anchor |

## Front pages

Overlay on screen, never printed.

| Token | Value | Use |
| --- | --- | --- |
| `ecad.front.derived` | ACI 5 (blue), dashed | Outline of a derived cover page block |
| `ecad.front.typed` | ACI 30 (orange), dashed | Outline of the Notes block |

## PLC visuals

| Token | Value | Use |
| --- | --- | --- |
| `ecad.plc.tile_width` | 196 px | Tile width in Lists ▸ PLC |
| `ecad.plc.square` | 13 px | Channel square |
| `ecad.plc.square_gap` | 4 px | Between squares |
