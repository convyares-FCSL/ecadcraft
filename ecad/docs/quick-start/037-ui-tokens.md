# Brief 037: ui tokens

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 037 (UI tokens, primitives and fonts, U1), on `feature/ui-tokens`, not yet in `dev`:**
interface text is drawn in **IBM Plex Sans** (embedded, so present on every machine), and egui's
widgets take the Graphite shell colours from the one token table (`ecad/docs/design/tokens.md`).
The layout is unchanged; panels look slightly different (font, colours, 13 px body text). Unfold
**Developer** at the bottom of the PROJECT panel and click **ui.primitives** to see every primitive
(buttons, icon buttons, pills, chips, badges, fields, segmented control, key caps, rows, read and
provisional values) in every state, with the shell colours and the type ramp; compare it with the
spec's Tokens board. Barlow Semi Condensed is embedded for the sheet but not drawn yet (U4a).
- *Automated checks:* `ecad/crates/ecad-tokens/tests/all/` (tokens.md equals the constants,
  contrast, what prints, cap heights), `ecad/crates/ecad-ui/tests/all/` (each primitive found by
  its label, the disabled reason, the focus ring, read values, icons, embedded fonts and their
  hashes), `ecad/xtask` (`layers`: the UI kit's dependencies, no local styling) and CADCraft's
  `crates/ui-egui/tests/font_definitions.rs` (seam US2).
- *Agent-observed:* the worker took a screenshot of the showcase in the trial; no one has clicked
  through it.
