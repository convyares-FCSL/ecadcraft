# Brief 009: Cable look and feel, and core designation

Status: Draft (overseer, 2026-11-07, from the owner's feedback on brief 005)
Branch: `ecad/cable-look`
Designer: **yes**, first (`ecad/docs/design/009.md`) · Size: L
Depends on: 004, 005

## Goal

Cables look like a cable on an electrical drawing, not a thin fan, and every core carries its
designation. This is core look and feel for cables, not part of the later general tidy-up.

The owner's reference (2026-11-07): a cable label box (cable number, designation, type such as
`4/C #10`), one thick cable line from it, a thick vertical spine, curved bends where each core
leaves the spine, then a thin conductor tail to its terminal. The thick part is the cable; the thin
tail is the wire.

## Owner direction (2026-11-07)

1. **Tails.** Each conductor leaving the cable has a tail long enough to carry its labels.
2. **Cores.** A cable has a cable number; each core has a core number or colour.
   - Traditional view: the core label sits near where the cable slash crosses the wire.
   - Fan view: the core label moves to the start of the tail, with the wire number.
   This ends the deferral of core designation from brief 004 (`CableMember.core` exists in the
   model and is always `None` today).
3. **Section styling.** Thick lines (trunk, spine) show the cable section; curved bends and thin
   lines show the wire section.
4. **One-sided fan.** A cable may fan at one end only. The other end finishes at a reference marker
   pointing to another area or page; on that page the cable starts from a reference marker and fans
   back out into wires and components.

## Scope

1. Fan view restyled to the owner's reference: label box, thick trunk and spine, curved bends, thin
   tails sized for labels. Drawn by the ECAD projection (design 005 O1), so it snaps, plots and
   exports as shown.
2. Fan at either end, one end or both ends, per cable end.
3. A cable-end reference marker (text only in this brief: the target area or page the user types).
4. Core designation:
   - set and change a member's core number or colour (interactive and JSON forms, identical
     transactions);
   - shown in both views as in direction 2;
   - the hover, debugger trail and `ecadinspect` show it.
5. Update the quick start with a click-only example, and new reference screenshots.

**Stop and report (owner gate):**
- If changing a core label on an existing cable needs a new op or any `.ecad` format change (E4, E5).
- If the one-sided-fan choice per end needs to be stored in the project rather than as view state:
  propose where it lives.

**Out of scope:**
- Linking the two halves of a cable across pages into one cable with live cross-references (the
  "cable by tag across pages" deferral from brief 004): a separate brief.
- Core numbering schemes or automatic numbering; the cable schedule; spares.
- The general tidy-up (defaults, sizes across the app).

## Acceptance criteria

1. Fan view matches the approved design: label box, thick trunk and spine, curved bends, thin tails;
   reference screenshots approved by the owner.
2. A cable fans at one end, the other end or both, and the one-sided end shows its reference marker.
3. Setting a core label is one transaction; undo and redo restore it; save and reopen keep it;
   replay equals the live state.
4. The core label appears at the slash crossing in traditional view and at the tail start in fan
   view, next to the wire number.
5. The JSON and interactive forms for core labels give identical transactions.
6. Hostile input (long or empty labels, NaN geometry, a 20-core cable) never panics.
7. Switching views stays presentation only (brief 005 criteria 1, 2 and 4 still pass).

## Needs owner check

Whether the cable reads like a real drawing on Windows, and where labels sit at small zoom.

## Questions for the owner

1. Is the label box part of this brief, or only the line work?
2. Core colour: free text (`BK`, `BN`, `GNYE`), or a fixed list?
3. The reference marker: free text now, live cross-reference later. Is that split right?
