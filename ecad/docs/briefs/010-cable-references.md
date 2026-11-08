# Brief 010: Cable references across pages

Status: Approved (owner, 2026-11-07, to be built in parallel with 009 and 011)
Branch: `ecad/cable-references`
Designer: **yes**, first (`ecad/docs/design/010.md`) · Size: L
Depends on: 004, 005 (branch from `ecad/cable-views` until PR #24 is merged). Runs in parallel
with 009, which draws a free-text reference marker at a one-sided cable end; this brief supplies
the live link behind it.

## Goal

One cable can be drawn on several pages or areas. Where the drawing of a cable stops on one page, a
reference shows where it continues, and the continuation on the other page points back. The
references are live: renumber or reorder pages and nothing breaks (concept spec: "Renumber 200
pages and nothing breaks").

The owner's use (2026-11-07): a cable fans in from wires on one page, runs to a reference to
another area or page, and from a reference there fans back out to wires and into components.

## Principles that apply

- References are by ID, never by tag or page name (P2, DOC-02).
- Cross-reference text is a derived, live field, never stored as fact (concept spec, "Nets,
  cross-refs, BOM and diagnostics are never stored as fact").
- An unmatched reference is an orphan and gets a red marker (concept spec).

## Scope

1. **One cable, several drawings:** a cable may have more than one representation, on different
   pages or areas, each holding the members drawn there. The designer and worker propose how a
   second drawing of an existing cable is made (for example a slash on the second page that picks
   the existing cable, or a "continue cable" command), in one transaction, with JSON and
   interactive forms that record identical transactions.
2. **Reference ends:** each drawing of a cable can end at a reference instead of a fan or terminal.
   The reference shows derived text naming the other drawing's page and position (format proposed
   by the designer, for example `/12.4`), updated live when pages are renamed, reordered or moved.
3. **Navigation:** hovering a reference previews its target; a click or a typed command opens the
   target page with the other end selected.
4. **Orphans:** a reference whose other end was erased or never drawn shows a red marker and a
   diagnostic.
5. Hover, debugger trail and `ecadinspect` show every drawing of a cable and its references.
6. Quick start: a worked click-only example across two pages; reference screenshots.
7. MCP (brief 011 rule): each new or changed command has an MCP contract test when 011 is merged; if
   011 is not merged yet, write the test so it drives the JSON form and can be moved later.

**Stop and report (owner gate):**
- Any `.ecad` format change or new op (E4, E5). A cable with several representations may already be
  allowed by the model (DOC-05 limits an item to one representation, not a subject); check first.
- Anything outside `ecad/`.

**Out of scope:**
- Off-page references for single wires and potentials (nets): a later brief.
- Automatic matching by tag across pages (inference): later.
- The look of the cable and the drawing of the marker itself: brief 009. Here the marker is the
  minimal drawing needed to show and click the reference; 009's styling takes over at merge.

## Acceptance criteria

1. A cable drawn on two pages is one cable: same ID, members split between its drawings, and
   `derive::nets` unchanged by splitting it.
2. Each reference shows the other end's page and position. Reordering or renaming pages updates
   the text with no new transaction.
3. Erasing one drawing makes the other end an orphan with a red marker; undo restores the link.
4. Save, reopen and replay give the same cable, drawings and references.
5. Hover previews the target; click or command opens it with the other end selected.
6. The JSON and interactive forms give identical transactions; hostile input never panics.
7. Headless UI test by label and approved reference screenshots of both ends.

## Needs owner check

Whether the reference reads like a real drawing, and whether navigating feels right on Windows.

## Decisions taken while the owner was away (overseer, 2026-11-07)

The owner asked for 009, 010 and 011 to run in parallel while away. Design questions are decided by
the overseer on the designer's recommendation and listed in the PR for the owner to confirm or
change before merge.
