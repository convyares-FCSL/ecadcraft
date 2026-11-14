# Brief 046: Graphical terminal-strip pages

Status: Approved (owner, 2026-11-13: all 12 design §14 questions answered "as recommended"; no FP
needed, design §12)
Branch: `feature/strip-pages`, from `origin/dev` `be6f4413` (036 multi-level terminals and the UI
programme 037–043 merged).
Designer: **yes** (`docs/design/046.md`) · Size: L (the roadmap's estimate; the design keeps it to
one new module and two lane-U files)
Roadmap: `docs/discussions/roadmap-review-2026-11.md` M1 item **1.8**, lane U, second (after 1.0,
the live bridge). Decided there: owner question 19 (one derived plan, two page styles, the project
chooses which goes in the book). Reads 028's strip model, 030's accessories and links and 036's
levels; writes only lane U files (§3.2: "it writes only `generated.rs` and the page styles").

## Goal

Every terminal strip can be printed as it is wired on the rail: a horizontal row of terminals, the
inside conductors going up to their targets and the outside conductors going down, with wire
numbers, the terminal's own page reference, cable boxes with core colours, levels, links,
bridges and accessories. It is drawn live from the same rows as 028's terminal plan, as a second
style of the same generated `terminals` page, never stored and never a second model.

## What exists today (`origin/dev` `be6f4413`)

- **028:** a generated page of kind `terminals` draws the tabular plan, every strip (each from a new
  sheet) or the one strip named by its `strip` property; 44 rows per A3 sheet; kept sheet counts
  per strip (`StripData.sheets`, `Strips.sheets_total`).
- **030:** gap accessories (end stop, end plate, partition, strip label), markers, and links in
  lanes (`StripData.acc`, `links_at`); the plan in physical order (`StripData::plan`).
- **036:** one plan row per level, grouped by position; shown labels (`1.L`), types, free and
  spare levels, two-point levels that break the net, internal bridges, ratings.
- **PR #75 (O1):** a shown terminal-plan page redraws only when a strip it shows is in
  `Strips::last_changed`; generated pages join the commit's touched set.
- **031 / 040:** text kinds and the defaults registry; generated content is ink on the locked
  `ECAD-GENERATED` layer, drawn in Barlow Semi Condensed; overlay never prints.
- Print Book and Release draw generated pages through `generated::parts_of_pending`, so a new page
  style prints with no Publish change.

## Scope

1. **A second page style** of the `terminals` page: `graphic`, chosen per page, with a project
   default (registry entry `terminal.plan.style`, built-in `table`). The table style is unchanged.
2. **The drawing** (design §3–§5): one column per plan row (level) along a horizontal strip;
   inside above, outside below; targets, wire numbers, the terminal's own place, core labels,
   cable bars and cable boxes with the cable's type and far device; PE, spare, free and two-point
   levels; links and wire bridges in lanes; end stops, end plates, partitions, strip labels and
   markers; a strip tag box.
3. **Paging** (design §6): each strip from a new sheet; as many columns as fit; a position never
   split; links and cables that run on draw their continuation; per-sheet titles `-X01 (2/3)`.
4. **Text sizes from the text styles** (design §7): existing kinds only, no new kind.
5. **Update map** (design §9): only strips whose drawing changed are redrawn; the shown page redraws
   only when it shows one; sheet counts kept, not walked.
6. **Pages and Publish** (design §8): New page ▸ **Terminal strips**; switching a page's style;
   Print Book and Release unchanged.
7. **MCP** for the changed command (`newpage` gains `style`); bench rows.

Out of scope: new panels or Lists views (UI polish comes later), clicking through from a strip page
to the terminal, colour per text role (the reference's coloured texts), panel layout views, a
strip page laid out vertically, editing anything on the page, and the terminal manager (2.6).

## Format and ops

No new op, no new record field, no new page property name. The style is a registry entry
(`terminal.plan.style`, levels project and page) stored by 031's existing registry mechanism; the
overseer confirms that this needs no FP (design §12).

## Acceptance criteria

The full table is design §11. In short:

1. Fixture G (design §11): a `terminals` page with `style: graphic` draws one column per level of
   `-X01`, inside targets above and outside below, wire numbers, places, and the strip tag box.
2. `-W01` (`7G1.5`, five cores on `-X01`) draws one bar under its five cores and one cable box with
   its tag, type and far device; core labels show beside each core.
3. A 40-position strip runs onto a second sheet with no position split; a link across the break
   draws its continuation stub on both sheets; titles read `(1/2)` and `(2/2)`; the book's Total
   counts both sheets.
4. L/N/PE, fuse and spare positions, end plates, partitions, markers and links draw as design §4–§5
   say.
5. Text heights follow the text styles: changing `text.table.height` changes the terminal labels
   on the shown strip page.
6. Update map: attaching a wire to `-X01:3` redraws the shown strip page once; a commit that touches
   no shown strip projects nothing; changing `-W01`'s type redraws only the strips its cores are
   on (counters, verify-mode equality of kept sheet counts).
7. Print Book with a graphic page prints the same parts as the page shows, in ink.
8. MCP contract test (brief 011 rule): `newpage {generated: "terminals", style: "graphic"}` records
   `Author::Agent(<client>)` and is refused with writes off; `setdefault` of `terminal.plan.style` at page
   level records one transaction.
9. Bench rows of design §10 within their targets.
10. A 036 project opens unchanged; its `terminals` pages still draw the table.
11. Reference screenshots, approved by the owner.

## Process

No tester (light process): no format, op, movement or store change. Reviewer once, on the
feature PR. Per-lane files (owner, 2026-11-13): `docs/quick-start/046-strip-pages.md`,
`tests/all/lanes/lane_046.rs` in `ecad-cadcraft` and `ecad-mcp`, tokens in design §13.

## Needs owner check

- ~~Approve the design and answer its questions (§14).~~ Done 2026-11-13: the owner answered all
  12 "as recommended".
- A click-only trial: `docs/quick-start/046-strip-pages.md`.
