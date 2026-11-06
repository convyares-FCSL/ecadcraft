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
| `005-fan.png` | The same cable in fan view after **Cable View** and a click on the slash: one magenta trunk from (5,5) to (55,5) and three magenta fan lines at each end to the terminals; wires, slash and crossing dots gone; rings and dots unchanged; `Cable C1 now shown in fan view.` echoed (brief 005) | pending owner approval |

Rendered at 1280×800, dark theme, by `ac6_reference_screenshots` in `../mouse_tester.rs`.
Regenerate with `ECAD_TESTER_SHOTS=<folder> cargo test -p ecad-trial --test mouse_tester ac6`
(needs a wgpu adapter; in CI the test only checks pixels and writes nothing).

The 004 images are rendered by `ac9_cable_slash_by_label_and_reference_screenshots` in
`../cable_slash_tester.rs`; regenerate with
`ECAD_TESTER_SHOTS=<folder> cargo test -p ecad-trial --test cable_slash_tester ac9`.

The 005 images are rendered by `ac5_cable_view_by_label_and_reference_screenshots` in
`../cable_view_tester.rs`; regenerate with
`ECAD_TESTER_SHOTS=<folder> cargo test -p ecad-trial --test cable_view_tester ac5`.
