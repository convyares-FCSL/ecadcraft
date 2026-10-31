# Brief 005: Traditional / fan cable view

Status: Draft (overseer, 2026-10-31)
Branch: `ecad/cable-views`
Designer: **yes**, first (`ecad/docs/design/005.md`) · Size: M
Depends on: 004

## Goal

One key or button switches a cable between **traditional** and **fan**:
- **traditional:** every conductor drawn as its own wire, with the cable shown by its slash;
- **fan:** the conductors merge into one cable line that fans out at each end.

The switch is **presentation only**. It adds nothing to the history, never changes conductors, cable
membership or connections, and is instant.

## Scope

1. A view setting held as UI state, outside the ledger (P5). It must never move the head (SAVE-03),
   so it does not count as an unsaved change.
2. The fan view is drawn from the cable and its conductors' routes as a derived overlay or projection
   (the designer and worker propose which; Part B is provisional). The trunk route and the fan-out
   points are computed, not stored.
3. Editing while in fan view:
   - **default:** the fan drawing is read-only, and a pick or edit there is refused with "switch to
     traditional view to edit";
   - anything more needs the owner's decision.
4. Scope of the switch (per cable, per page, or global) and whether it is remembered between sessions
   follow review question 7. If remembered, it is stored outside the `.ecad` history (P5), for
   example in a per-user settings file, never as a transaction.
5. Update the quick-start guide.

**Out of scope:**
- a stored "cable view" representation (§18.4: cable view persistence is deferred);
- the cable schedule output (the concept spec's "done when" also needs this; propose it as a separate
  brief once 004 and 005 land);
- orthogonal tidy-up.

## Acceptance criteria

1. Switching both ways leaves the ledger head, the log, every ID and `derive::nets` unchanged. A
   contract test compares document and log before and after 10 switches.
2. The project does not become unsaved by switching. Save, reopen, and the conductors and cable are
   unchanged.
3. A cable of 1, 2 and 20 conductors, with conductors on different routes, draws in fan view without
   panic. A conductor with a loose end draws sensibly per the spec.
4. Undo and redo after a switch act on the last real edit, never on the switch.
5. Approved reference screenshots exist for both views of the same cable. A headless UI test toggles
   by the control's element name.
6. Edits refused in fan view leave the head unchanged and give the spec's message.

## Needs owner check

Whether the fan view reads well and the switch feels instant on Windows.
