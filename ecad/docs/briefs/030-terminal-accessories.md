# Brief 030: Terminal strip accessories

Status: Draft, revision 2 (designer, 2026-11-12, after the owner's answers to revision 1: every
accessory is placed by the user; the jumper model is open, design §5, owner to decide)
Branch: `feature/terminal-accessories`, from `origin/feature/terminal-strips` `81139aaf` (028, in
review as PR #55). The worker starts after 028 merges into `dev`, then merges `dev` into this
branch first.
Designer: **yes** (`docs/design/030.md`) · Size: M (S without jumpers)
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
4. **Jumpers: owner to decide** between the three models in design §5 (a marked bridge conductor;
   a strip property shown only in the plan; the strip property plus derived marks on the
   schematic). The worker does not build jumpers until the owner has chosen; the rest of 030 does
   not wait.
5. **Checks** rows with a way out: an accessory after a position that no longer exists, accessories
   with no part number; with jumpers, a jumper across a partition (a warning, never a refusal).
6. **Update maps** for every derived row; **MCP** for every new or changed command; **bench** rows.

## Later (not designed now)

- **Automatic accessories:** end stops at both ends, an end plate on the open side and where the
  terminal family changes, from a rule with per-gap overrides, and the checks that need such a rule
  (a strip with no end stop, an open end with no end plate).
- Splitting jumpers by hand (if option A is chosen), level jumpers for multi-level terminals.

Out of scope: test plugs and shield clamps, rail lengths and rail parts, panel layout (mounting)
views, printing marker text, a manufacturer's catalogue (libraries, stage 3).

## Format and ops

No new op and no new record field. Everything is an ordinary optional property (design §14,
**the overseer decides FP1–FP4**): strip keys `acc.start`, `acc.end`, `acc.after.<label>`,
`acc.on.<label>`; accessory part keys `<item>.part_number|maker|description` and
`marker.per_card`; project keys `strip_<item>_<field>`. Renumbering terminals moves the `acc.*`
keys in the same transaction (FP4, ops recorded, not format). Jumper keys follow the owner's choice.

## Acceptance criteria

1. Fixture A (design §12): with nothing placed, the plan and BOM show no accessories. Placing end
   stops, end plates, a partition and markers with `accessory` adds plan rows in position order and
   BOM rows per part, one transaction each (a list of positions is one transaction); `u` removes
   them.
2. Renumbering a strip's terminals moves its `acc.after.*` and `acc.on.*` keys with the terminals,
   in the same transaction.
3. Markers are on the BOM only with a marker part number, as `ceil(markers / per_card)` cards when
   a card size is known.
4. Every Checks row of design §8 (except the jumper rows until §5 is decided) has a working fix or
   go-to.
5. Changing one strip's accessory property recomputes only that strip; a project default
   recomputes only the strips that inherit it; every other op leaves accessories alone (verify
   mode equality, counter).
6. MCP contract tests (brief 011 rule): `accessory`, `setprop` of `acc.*` and accessory part keys,
   `projectprops` with `strip_*` and `renumber {kind: "terminals"}` with accessory keys record
   `Author::Agent(<client>)` and are refused with writes off; `stripplan`, `bom` and `checks` work
   with writes off and record nothing.
7. Bench rows of design §11.3 within their targets; `stripbuild` does not grow by more than 10 %.
8. Save and reopen keep everything; a 028 file opens unchanged and shows no accessories.
9. Reference screenshots approved by the owner.
10. Jumpers: acceptance criteria are added when the owner chooses the model (design §5).

## Needs owner check

- Choose the jumper model (design §5, §15 Q1), after checking the drawing pack.
- Approve the rest of the design and answer §15 Q2–Q3.
- A click-only trial: quick start 5O.
