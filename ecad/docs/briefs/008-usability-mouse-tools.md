# Brief 008: Usability, ECAD tools by mouse

Status: Approved (owner, 2026-11-01)
Branch: `ecad/usability-mouse-tools`
Designer: **yes**, first (`ecad/docs/design/008.md`) · Size: M-L

## Goal

The owner can build the quick-start example (a component with a terminal, a placed coil and a wire
between them) **by clicking**, with no handles, IDs or JSON typed. Every step is visible on the
drawing as it happens.

## Why

Today every ECAD command is JSON-only: adding a terminal means selecting an edge, typing `list` to
read its handle, then typing `addterminal {"handle":…,"at":[x,y]}`. The owner could not follow the
quick-start trial on Windows for this reason (2026-11-01). The ECAD Tool Sets buttons only fill in
a JSON template. This brief is about how things are entered, not about new electrical meaning.

## Scope

Give the existing commands an interactive form (CADCraft's `CommandSpec` prompt machine: prompts,
picks, object snaps, previews, Enter/Esc), used when they are started from the Tool Sets tab, a menu
or by typing the bare command name. The JSON form stays exactly as it is for scripts, tests, the
control channel and MCP: programmatic calls never prompt.

1. **Add Terminal:** pick the component's edge, or snap to a point on it; the terminal goes there.
   Repeat until Enter. A name label is optional (Enter accepts a default, e.g. 1, 2, 3).
2. **Place Symbol:** the symbol follows the cursor as a preview; click to place; a key rotates by
   90° (and mirrors) before the click. Repeat until Enter.
3. **Wire:** click a start point; each click adds a bend; ends snap to terminals and show that they
   will attach before you click; Enter or a click on a terminal finishes. Esc cancels without a
   change.
4. **Connect / Disconnect:** pick the wire end, then the terminal (Connect) or just the end
   (Disconnect).
5. **Convert to Component:** as now (selection, then the button), with "select objects" prompting
   if nothing is selected.
6. **Seeing what is there:** terminals and attached ends are clearly visible at every zoom;
   hovering a component, terminal or wire shows its name/ID and its connections (tooltip or the
   existing debugger panel), so nobody needs `list` or `ecadinspect` to find an ID.
7. The ECAD Tool Sets buttons start these interactive forms instead of filling in a JSON template.
   Set Property, Add to Cable, Delete, New Page and Open Page may keep templates in this brief.
8. Update `ecad/docs/quick-start.md` so §5–§7 are done by clicking, with a native Windows variant.

**Out of scope:** new electrical semantics (numbering, cables by slash: brief 004, libraries); a
symbol library or picker beyond the built-in coil; changes to the file format or the transaction
model. If an interactive form needs anything new from CADCraft (for example, snapping to ECAD
terminals), stop and report: a new seam needs the owner's approval.

## Acceptance criteria

1. Each interactive command records **exactly the same transaction** as its JSON form with the same
   resolved values (a contract test per command compares the two).
2. Esc at any prompt leaves the document, head and selection unchanged; undo after a finished
   interactive command undoes it in one step.
3. Headless UI tests (egui_kittest, by element name) drive the quick-start example end to end by
   clicks and keys: rectangle, convert, two terminals, place rotated coil, wire terminal to
   terminal; the result equals today's quick-start test result.
4. Wire ends snap to and attach to terminals within the snap aperture; a click near but outside it
   does not attach, and the preview shows the difference before the click.
5. Hover shows the object's name or ID and its connections; nothing is written to the document by
   hovering.
6. Approved reference screenshots for: terminal markers, the wire preview with a snapping end, the
   placed coil preview.
7. `cargo xtask ci` green on Linux and Windows; never-crash rules hold for every prompt input
   (hostile coordinates, NaN, huge counts).

## Needs owner check

The owner repeats the quick-start example on native Windows by clicking only, and says whether it
is easy enough. That trial is this brief's real acceptance.
