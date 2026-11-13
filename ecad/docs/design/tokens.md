# ECAD design tokens

Names used by ECAD design specs (`docs/design/<brief>.md`). ECAD draws inside CADCraft's UI, so
everything not listed here comes from CADCraft's theme (`../crates/ui-egui/src/theme.rs`,
`theme::Tokens`) and is referred to as `cad.<token>` (for example `cad.snap`, `cad.hover`,
`cad.panel`). ECAD never overrides CADCraft's own tokens.

Visual language: traditional 2D CAD. Dark canvas (CADCraft's `cad.canvas`), thin vector lines in
the classic ACI colours, utilitarian panels, monospaced labels, square corners, no gradients,
shadows or rounded web-app styling.

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
