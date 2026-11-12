# Brief 030: Terminal strip accessories

Status: Draft, revision 3 (designer, 2026-11-12; decisions in design §17: every accessory is
placed by the user; jumpers are strip-owned links (model D); FP1–FP4 approved. The hybrid Jumper
tool is confirmed. Open: design §15 Q2–Q3 (owner) and FP5–FP7 (overseer))
Branch: `feature/terminal-accessories`, from `origin/feature/terminal-strips` `81139aaf` (028, in
review as PR #55). The worker starts after 028 merges into `dev`, then merges `dev` into this
branch first.
Designer: **yes** (`docs/design/030.md`) · Size: M
Roadmap: `docs/roadmap-v0.1.md` F5, lane B, after 028. 027 Release exports the terminal plan, so
its CSV gains the accessory rows (design §9.3).

## Goal

A terminal strip as it is bought and built: end stops, end plates, partition plates, markers, a
strip label and jumpers, each **placed by the user** where they want it. They appear in the
terminal plan in position order and on the BOM, derived live, with nothing typed twice and nothing
added that the user did not place.

## What exists today (028, `81139aaf`)

- A strip (`-X01`) is one device; each terminal is one symbol with one connection point. The
  strip's `terminals` offer (`12+2PE`) adds spare positions. Positions are sorted by label.
- A **bridge** is any conductor whose two ends are on two terminals of one strip (derived). Draw
  Bridge draws a staple; the wire report lists it as `Bridge -X01`; bridges are not on the BOM.
- The BOM gives one row per strip per terminal type. `Strips` is kept live by an update map and
  checked against a rebuild in verify mode.

## Scope

1. **Placed accessories.** End stops, end plates, partitions and strip labels in any gap (start,
   after a position, end); markers on any position. Stored as strip properties keyed by position
   label; placed with a new `accessory` command, a Strip Accessory tool and the Reports ▸ Terminals
   tab. Renumbering terminals moves the keys with their terminals.
2. **Part data** from strip properties, falling back to project properties (`strip_*`), until
   libraries (stage 3) supply the same fields.
3. **Terminal plan** rows for accessories in position order; **BOM** rows per strip per accessory
   part, only for what is placed; markers counted (as cards when a card size is known) only when a
   marker part number is known.
4. **Jumpers as links owned by the strip** (design §5): a link joins positions of one strip (a
   range or a skip list) into one net with one wire number, never a conductor. It is drawn
   automatically as a dashed line through its terminals on every page that shows them, with a
   continuation stub where it goes on elsewhere; overlapping and skip links run in lanes and never
   cross. One link set per strip. Links are made with a **Jumper tool** that feels like drawing a
   wire, and the line can be selected to extend, shorten, split or delete the link (the hybrid,
   confirmed by the owner; pure automatic links, design §5.7, were rejected). On the BOM, each link
   needs `ceil(span / bar size)` jumper bars; bar size per strip, project default, else 10.
5. **Checks** rows with a way out: an accessory after a position that no longer exists, accessories
   with no part number, a link across a partition (a warning, never a refusal), a link naming
   positions the strip does not have.
6. **Update maps** for every derived row; **MCP** for every new or changed command; **bench** rows.

## Later (not designed now)

- **Automatic accessories:** end stops at both ends, an end plate on the open side and where the
  terminal family changes, from a rule with per-gap overrides, and the checks that need such a rule
  (a strip with no end stop, an open end with no end plate).
- **Multi-level terminals and links per level**: a likely next brief, after the owner's status
  review. 030's links join whole positions.

Out of scope: test plugs and shield clamps, rail lengths and rail parts, panel layout (mounting)
views, printing marker text, a manufacturer's catalogue (libraries, stage 3).

## Format and ops

No new op and no new record field. Everything is an ordinary optional property (design §14,
**the overseer decides FP1–FP4**): strip keys `acc.start`, `acc.end`, `acc.after.<label>`,
`acc.on.<label>`; accessory part keys `<item>.part_number|maker|description` and
`marker.per_card`; project keys `strip_<item>_<field>` (FP1–FP4, approved). Renumbering terminals
moves the `acc.*` keys in the same transaction (FP4). For jumpers (**FP5–FP7, the overseer
decides**): strip keys `link.<n>` and `link.<n>.part_number|maker|description`; strip `jumper.bar`
and `jumper.part_number|maker|description` with project `strip_jumper_*`; a link edit records the
numbering pass's wire-number ops in its transaction, and renumber rewrites `link.<n>` values.
022's connectivity reads strip links as a bridge source (code, not format).

## Acceptance criteria

1. Fixture A (design §12): with nothing placed, the plan and BOM show no accessories. Placing end
   stops, end plates, a partition and markers with `accessory` adds plan rows in position order and
   BOM rows per part, one transaction each (a list of positions is one transaction); `u` removes
   them.
2. Renumbering a strip's terminals moves its `acc.after.*` and `acc.on.*` keys with the terminals,
   in the same transaction.
3. Markers are on the BOM only with a marker part number, as `ceil(markers / per_card)` cards when
   a card size is known.
4. Every Checks row of design §8 has a working fix or go-to.
5. Changing one strip's accessory property recomputes only that strip; a project default
   recomputes only the strips that inherit it; every other op leaves accessories alone (verify
   mode equality, counter).
6. MCP contract tests (brief 011 rule): `accessory`, `jumper` (make, extend, shorten, split,
   delete), `setprop` of `acc.*`, `link.<n>`, `jumper.bar` and accessory part keys, `projectprops`
   with `strip_*` and `renumber {kind: "terminals"}` with accessory keys and links record
   `Author::Agent(<client>)` and are refused with writes off; `stripplan`, `bom` and `checks` work
   with writes off and record nothing.
7. Bench rows of design §11.3 within their targets; `stripbuild` does not grow by more than 10 %.
8. Save and reopen keep everything; a 028 file opens unchanged and shows no accessories.
9. Reference screenshots approved by the owner.
10. The Jumper tool (pick 1, then 4, Enter) makes `link 1-4` in one transaction: 1–4 share one wire
    number, and every page showing those terminals draws the link line, with a stub where the link
    continues elsewhere; extend, shorten, split and delete from the selected line are one
    transaction each; renumbering keeps each link on its terminals.
11. Jumper BOM: `ceil(span / bar size)` bars per link, bar size per strip, else the project's, else
    10; a link's own part number gives its own row.

## Needs owner check

- Answer design §15 Q2–Q3.
- Approve the rest of the design.
- A click-only trial: quick start 5O.
