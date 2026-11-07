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
| `010-ref-source.png` | Fixture R of design 010 (§11), page 1 "Supply" after the continuation and the connects: three wires from K1, the slash with magenta crossing dots, no end marks at x = 60, a magenta ring at (65,5) and the derived text `/2.1` to its right (brief 010) | pending owner approval |
| `010-ref-target.png` | Page 2 "Field": the three continued wires from the reference to K2's terminals (green dots), the slash across them, a magenta ring at (20,5) and `/1.2` to its left (brief 010) | pending owner approval |
| `010-ref-orphan.png` | Page 1 after Field's reference was erased: red ring and red `/?` (brief 010) | pending owner approval |
| `010-ref-hover.png` | Hover lens on page 1's reference: `Cable reference  /2.1`, this end and the other end (page 2 "Field", column 1, 3 wires) and one `-> K2:…` line per wire (brief 010) | pending owner approval |
| `010-continue-preview.png` | Continue Cable on page 2 before the second click, cursor at (40,5): three stubs and the slash previewed, the ring with the live `/1.2`; CADCraft's dynamic-input box covers the `-> 3 wires continue` preview text (brief 010) | pending owner approval |

Rendered at 1280×800, dark theme, by `ac6_reference_screenshots` in `../mouse_tester.rs`.
Regenerate with `ECAD_TESTER_SHOTS=<folder> cargo test -p ecad-trial --test mouse_tester ac6`
(needs a wgpu adapter; in CI the test only checks pixels and writes nothing).

The 004 images are rendered by `ac9_cable_slash_by_label_and_reference_screenshots` in
`../cable_slash_tester.rs`; regenerate with
`ECAD_TESTER_SHOTS=<folder> cargo test -p ecad-trial --test cable_slash_tester ac9`.

The 005 images are rendered by `ac5_cable_view_by_label_and_reference_screenshots` in
`../cable_view_tester.rs`; regenerate with
`ECAD_TESTER_SHOTS=<folder> cargo test -p ecad-trial --test cable_view_tester ac5`.

The 010 images are rendered by `fixture_r_by_label_and_canvas_clicks_with_reference_screenshots` in
`../cable_references_tester.rs`; regenerate with
`ECAD_TESTER_SHOTS=<folder> cargo test -p ecad-trial --test cable_references_tester`.
