# Brief 004: Wires to cable, the slash gesture

Status: Draft (overseer, 2026-10-31)
Branch: `ecad/cable-slash`
Designer: **yes**, first (`ecad/docs/design/004.md`) · Size: M-L
Depends on: 002 (saves are safe on Windows before new visible features)

## Goal

The user draws a short slash across existing wires, and that creates one cable that groups exactly
the conductors it crosses. The cable is visible as the slash and can be undone, saved, reopened and
inspected like any other edit. This is the first visible test of the model: meaning (cable,
conductors) separate from drawing (the slash).

## What already exists

- The model:
  - `Cable { properties, members: Vec<CableMember { conductor, core: Option<String> }> }`;
  - ops `CreateCable`, `DeleteCable`, `AddCableMember`, `RemoveCableMember`;
  - `Subject::Cable` for representations.
- The command `cableadd {cable?, conductors, tag?}` (`crates/ecad-cadcraft/src/commands.rs:44`,
  `:443`).
- Cable membership changes no conductor (DOC-04), and is covered by S10.

So no file-format change should be needed. If one is, stop and report: it would be a schema change
under E5 and E4.

## Scope

1. A `cableslash` command (the id is the designer's call, namespaced in Tool Sets as `ecad.…`):
   - interactive: two points defining the slash;
   - JSON form: `{from: [x,y], to: [x,y], tag?}`;
   - it finds the conductors whose route segments the slash crosses, ordered along the slash from
     `from` to `to`;
   - in **one** transaction it creates the cable with those members and a cable representation (the
     slash geometry) linked to it.
2. The rules for what counts as crossing, all listed in the spec and tested:
   - a plain intersection with a route segment counts;
   - the same conductor crossed twice is added once;
   - an ordinary line that is not a conductor is ignored and reported;
   - touching only at an end or a vertex: the designer proposes, the owner decides.
3. Refusals:
   - fewer than one conductor crossed;
   - a crossed conductor already in a cable is **refused**. "A conductor in several cables" stays
     deferred (§18.4);
   - a read-only project.
4. The slash is a representation of the cable:
   - erasing it removes the representation only (L5 and P6);
   - deleting the cable stays the explicit `DELETE`;
   - moving wires does not change membership.
5. An "ECAD" Tool Sets entry and the debugger trail: slash → cable → conductors.
6. Update the quick-start guide with a worked example.

**Out of scope:**
- core numbering or designation schemes (foundations: deferred). `core` stays `None`; members are
  ordered by crossing order only. This is the overseer's call, delegated by the owner on 2026-10-31;
- spares;
- inferring a cable by tag across pages;
- the cable schedule;
- the fan view (brief 005);
- copying cables (L3).

## Acceptance criteria

1. Slashing across N parallel wires creates one cable with exactly those N conductors, in crossing
   order, in one transaction. Conductor IDs and attachments are unchanged.
2. `u` removes the cable and the slash in one step, and `redo` restores both with the same IDs.
3. Save, close and reopen give the same cable, members, order and representation. Replay equals the
   live state (`check_project`).
4. The slash across a mix of conductors and plain lines takes only the conductors and reports the
   ignored lines.
5. Each refusal in scope item 3 leaves the head unchanged and gives a clear message.
6. Erasing the slash keeps the cable. Deleting the cable keeps every conductor.
7. The JSON form and the interactive form give identical transactions for the same points.
8. A hostile-input test: huge coordinates, NaN, and a slash crossing a capped number of conductors
   (`MAX_CABLE_MEMBERS`) never panic.
9. A headless UI test drives the ECAD tab entry by label, and an approved reference screenshot shows
   the slash.

## Needs owner check

The look of the slash and how it feels to draw on Windows. The tester lists anything else it cannot
see.
