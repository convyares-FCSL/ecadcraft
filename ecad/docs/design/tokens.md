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
| `ecad.loose` | 1 (red) | `#FF0000` | A wire end attached to nothing |
| `ecad.attach-preview` | 3 (green) | `#00FF00` | "Will attach" ring and name while a command runs |
| `ecad.preview` | — | CADCraft's rubber-band colour | Rubber band, symbol preview (as CADCraft draws previews) |

Drawn geometry (rectangles, wires, symbol graphics) keeps its own layer colour: ECAD does not
recolour objects.

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
| `ecad.ring-size` | 16 px diameter | Attach-preview ring |
| `ecad.aperture` | CADCraft `APERTURE` setting (default 10 px) | How close the cursor must be for an ECAD capture |

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
