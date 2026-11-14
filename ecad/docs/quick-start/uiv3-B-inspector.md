# UI rev 3, section B: the inspector

Boards 1.05 (Device), 1.06 (Layout) and 1.07 (Checks) of the rev 3 handoff
(`docs/discussions/ui-2026-11/rev3-build-spec.md`, "Inspector (right panel in Draw)"). This is a
UI-only change: it adds no new behaviour.

## Try it

1. Open the demo project (`cargo xtask demo`, then open `demo.ecad`). Show page 2.
2. Click the coil of -K01. Press **Device** on the right rail.
   - The header shows `-K01`, `Pump contactor · coil A1–A2 · 2-A-5`, a **Row** button and collapse.
   - Then a status pill (`No checks on -K01 · only this device's own`), and the sections Identity,
     Parts, Where it is drawn, Connections, and Notes and attributes (folded). Click a section's
     title to fold it; a folded section shows a one-line summary.
   - Click a row of Where it is drawn to go to that contact.
   - **Row** opens Project ▸ Devices with -K01's row selected.
3. Press **Layout**: Placement (position, rotation and mirror are read from the drawing), Text on
   the sheet (each slot's On box, where, and size from the standard; the size is a list of the
   text styles), the contact table (Below, Right, Hidden), and Line and colour (folded).
4. Press **Checks**: This page or Project, each check as a card with its fix and a → go-to, then
   "Elsewhere in the project". The bottom bar's checks counter opens this tab.
5. Click empty paper (nothing selected): the header names the page and the Device tab shows the
   page's properties; Layout shows the defaults for the page.

## Not available yet (shown, does nothing)

- **+ Accessory** and **From catalogue** under Parts.
- Variant and Lock under Placement, and the contact table's **Used only / All contacts**.
- **Place** for a contact that is not drawn yet. Contacts not drawn show as "not used yet", with no
  button.

## Checks

`cargo test -p ecad-trial lane_uiv3_b`. Set `ECAD_TESTER_SHOTS` to a folder to write the
screenshots `uiv3-B-1.05-ours.png`, `-1.06-`, `-1.07-` and `uiv3-B-nothing-selected-ours.png`.
