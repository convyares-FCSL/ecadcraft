# Brief 030: Terminal strip accessories

Status: Draft (designer, 2026-11-12; design 030 §15 questions and §14 FP items await the owner and
the overseer)
Branch: `feature/terminal-accessories`, from `origin/feature/terminal-strips` `81139aaf` (028, in
review as PR #55). The worker starts after 028 merges into `dev`, then merges `dev` into this
branch first.
Designer: **yes** (`docs/design/030.md`) · Size: M
Roadmap: `docs/roadmap-v0.1.md` F5, lane B, after 028. 027 Release exports the terminal plan, so
its CSV gains the accessory rows (design §9.3).

## Goal

A terminal strip as it is bought and built: end stops, end plates, partition plates, jumpers and,
optionally, marker cards and a strip label. End stops and end plates come from a rule and can be
overridden; partitions and jumpers are placed by the user. All of them appear in the terminal plan
in position order and on the BOM, derived live, with nothing typed twice.

## What exists today (028, `81139aaf`)

- A strip (`-X01`) is one device; each terminal is one symbol with one connection point. The
  strip's `terminals` offer (`12+2PE`) adds spare positions. Positions are sorted by label
  (`natural_key`).
- A **bridge** is any conductor whose two ends are on two terminals of one strip (derived, no
  property). Draw Bridge (`bridge {from, to}`) draws a staple; the wire report lists it as
  `Bridge -X01`. The plan groups bridged terminals (`bridge_group`).
- The BOM gives one row per strip per terminal type (`part_number`, `part_number.pe`). Bridges are
  not on the BOM.
- `Strips` is kept live by an update map (design 028 §11.1) and checked against a rebuild in
  verify mode.

## Scope

1. **Automatic accessories.** Per strip, end stops at both ends and an end plate on the open side
   (and where the end-plate family changes), from a rule. A per-gap override replaces the rule at
   that gap; a project switch turns the rule off.
2. **Placed accessories.** Partition plates (and extra end stops or end plates) after any position,
   through a new `accessory` command, the Reports ▸ Terminals tab and a Strip Accessory tool.
3. **Jumpers.** A plug-in jumper is a bridge with conductor property `bridge = jumper`: the same net,
   no wire on the wire report, its own part per size (2-way, 3-way, 10-way…). Connected jumpers on
   one strip are one jumper; skip jumpers are allowed. Draw Bridge gains Jumper/Wire and a list of
   terminals.
4. **Part data** from strip properties, falling back to project properties (`strip_*`), until
   libraries (stage 3) supply the same fields.
5. **Terminal plan** rows for accessories in position order and jumper marks; **BOM** rows per
   strip per accessory part; optional **marker cards** and **strip label**.
6. **Checks** rows with a way out: jumper across a partition, overlapping jumpers, an open end with
   no end plate or end stop, an accessory after a position that no longer exists, accessories with
   no part number.
7. **Update maps** for every derived row; **MCP** for every new or changed command; **bench** rows.

Out of scope: multi-level terminals and their level jumpers, test plugs and shield clamps, rail
lengths and rail parts, panel layout (mounting) views, printing marker text, splitting one jumper
by hand, a manufacturer's catalogue (libraries, stage 3).

## Format and ops

No new op and no new record field. Everything is an ordinary optional property (design §14,
**the overseer decides FP1–FP6**): strip properties `acc.start`, `acc.end`, `acc.after.<label>`,
`open_side` and accessory part keys `<item>.part_number|maker|description` and `marker.per_card`;
project properties `strip_<item>_<field>` and `strip_accessories`; conductor property `bridge`.
Renumbering terminals moves `acc.after.<label>` keys in the same transaction (FP6, ops recorded,
not format).

## Acceptance criteria

1. Fixture A (design §12): a strip with project accessory defaults shows `end stop`, positions,
   `end plate`, `end stop` in the plan, and the BOM rows End stop ×2 and End plate ×1, with no
   edit. `strip_accessories off` removes them.
2. `accessory {strip, after: "4", items: ["partition"]}` adds a partition row after 4 and a BOM row
   in one transaction; `u` removes it; `items: null` restores the rule at that gap.
3. `bridge {terminals: [1, 2, 3], kind: "jumper"}` is one transaction: two conductors with
   `bridge = jumper`, one net, one 3-way jumper in the plan and on the BOM, and no row on the wire
   report. Setting `bridge` on a 028 wire bridge converts it both ways with one `u`.
4. A jumper across a partition is drawn, never refused, and gives a `jumper-across-partition`
   Checks row whose fixes (Make wire bridge, Remove partition) work.
5. Renumbering a strip's terminals moves its `acc.after.*` keys with the terminals they follow, in
   the same transaction.
6. Every Checks row of design §8 has a working fix or go-to.
7. Changing one strip's accessory property recomputes only that strip; a project default
   recomputes only the strips that inherit it; every other op leaves accessories alone (verify
   mode equality, counter).
8. MCP contract tests (brief 011 rule): `accessory`, `bridge` with `kind` and `terminals`,
   `setprop` of `acc.*`, `open_side`, accessory part keys and conductor `bridge`, and
   `projectprops` with `strip_*` record `Author::Agent(<client>)` and are refused with writes off;
   `stripplan`, `bom`, `wirelist` and `checks` work with writes off and record nothing.
9. Bench rows of design §11.3 within their targets; `stripbuild` does not grow by more than 10 %.
10. Save and reopen keep everything; a 028 file opens unchanged and shows automatic accessories.
11. Reference screenshots approved by the owner.

## Needs owner check

- Approve the design (`docs/design/030.md`) and answer its questions (§15).
- A click-only trial: quick start 5O.
