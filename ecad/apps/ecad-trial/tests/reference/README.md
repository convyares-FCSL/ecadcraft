# Approved reference screenshots

Reference images for UI checks, approved by the owner. Compare new renders against these before
changing how ECAD marks, previews or tooltips look; replace one only with the owner's approval.

| File | Shows | Approved |
| --- | --- | --- |
| `008-terminals.png` | Terminal rings on drawn and symbol components, green dots on attached wire ends (brief 008) | owner, 2026-11-01 |
| `008-wire-snap.png` | Wire being drawn: green capture ring and `-> A1` before the click (brief 008) | owner, 2026-11-01 |
| `008-place-preview.png` | Coil following the cursor before it is placed (brief 008) | owner, 2026-11-01 |
| `004-slash.png` | Fixture F after a cable slash: the slash across three wires, a magenta dot at each member crossing, none on the plain line (brief 004) | owner, 2026-11-06 |
| `004-slash-preview.png` | Cable slash before the second click: green rings on the three wires and the count text (brief 004) | owner, 2026-11-06 |
| `004-slash-refused.png` | Second slash across a wire already in a cable: red ring and refusal text before the click (brief 004) | owner, 2026-11-06 |
| `005-traditional.png` | Fixture G of design 005 after `zoom e`, cable C1 in traditional view: three wires between two drawn components, the slash with a magenta dot on each wire, terminal rings and green dots at both ends; ECAD tab open (brief 005) | pending owner approval |
| `005-fan.png` | The same cable in fan view after **Cable View** and a click on the slash, in brief 009's look (replaced 2026-11-07): the label box `C1` on a thick magenta trunk at y = 17.5, thick spines at x = 22.5 and 37.5, a curved bend and thin tail per wire to the terminals; wires, slash and crossing dots gone; rings and dots unchanged; `Cable C1 now shown in fan view.` echoed (briefs 005, 009) | pending owner approval (with the 009 images) |
| `009-traditional-cores.png` | Fixture H of design 009 after `zoom e`, cable C1 in traditional view with core labels `1`–`4` in magenta just past the slash crossings, above each wire (brief 009, design §6) | pending owner approval |
| `009-fan.png` | The same cable in fan view: label box `C1` / `MOTOR M1` / `4/C #10` on a thick trunk at y = 22.5, thick spines at x = 22.5 and 97.5, a thin curved bend and thin tail per wire, core label next to the spine and wire number further out at each tail start, one text height apart (brief 009, design §4; spacing per the overseer's review decision) | pending owner approval |
| `009-fan-one-sided.png` | Fixture H1 (no right-hand component, wires loose at x = 120) in fan view with a reference end (brief 010's model: one Point) at (125, 21.25): left end fanned, trunk running on to a flag pointing right with the placeholder text `->`; no spine, tails or loose-end squares on the right (brief 009, design §7) | pending owner approval |
| `009-fan-small-zoom.png` | `009-fan.png` zoomed out to a quarter (`zoom s 0.25x`): how labels read at small zoom (brief 009, needs owner check) | pending owner approval |
| `009-core-prompt.png` | **Core Label** started and W3 clicked: the prompt `Core number or colour for W3 <none>:`, with W3 highlighted in green (design §5.1 step 2) | pending owner approval |

Rendered at 1280×800, dark theme, by `ac6_reference_screenshots` in `../mouse_tester.rs`.
Regenerate with `ECAD_TESTER_SHOTS=<folder> cargo test -p ecad-trial --test mouse_tester ac6`
(needs a wgpu adapter; in CI the test only checks pixels and writes nothing).

The 004 images are rendered by `ac9_cable_slash_by_label_and_reference_screenshots` in
`../cable_slash_tester.rs`; regenerate with
`ECAD_TESTER_SHOTS=<folder> cargo test -p ecad-trial --test cable_slash_tester ac9`.

The 005 images are rendered by `ac5_cable_view_by_label_and_reference_screenshots` in
`../cable_view_tester.rs`; regenerate with
`ECAD_TESTER_SHOTS=<folder> cargo test -p ecad-trial --test cable_view_tester ac5`.

The 009 images are rendered by `check_a_core_label_by_label_and_reference_screenshots` and
`one_sided_fan_reference_screenshot` in `../cable_look_tester.rs`; regenerate with
`ECAD_TESTER_SHOTS=<folder> cargo test -p ecad-trial --test cable_look_tester`.
