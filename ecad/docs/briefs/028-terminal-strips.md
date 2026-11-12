# Brief 028: Terminal strips and cable schedule

Status: Draft (designer, 2026-11-12; owner questions in design 028 §14, FP items in §13 for the
overseer)
Branch: `feature/terminal-strips` (worktree `C:\dev\ecad-features\connectivity`, folder name
historical), from `dev` `dc2472c6` (025 merged).
Designer: **yes** (`docs/design/028.md`) · Size: M
Roadmap: `docs/roadmap-v0.1.md` F5, lane B (after F2 018, F4 022, F3 025). 027 Release (lane A)
exports this brief's cable schedule and terminal plan as CSVs.

## Goal

A terminal strip is a device (`-X01`) whose terminals (`-X01:1…n`) are placed on wires, linked by
bridges and numbered with 026's planner. A cable knows how many cores it has, which are used and
which are spare. A terminal plan and a cable schedule are derived from the drawing, shown live in
Reports and as generated pages, and never stored.

## What exists today

- **Terminal starter** (018): key `terminal`, prefix `X`, `Kind::Terminal`, one connection point
  `1` at its centre. Joining another terminal to `-X01` adds the next whole-number label. Both
  wires of a terminal land on that one point, so they are one net (022).
- **Bridges inside a definition** (022 FP5) join a device's own terminal keys; they are fixed per
  definition, so they cannot describe links between terminals of one strip.
- **Cables** (004, 009, 010): a cable groups existing conductors; each member has an optional
  core label (`cablecore`, duplicates allowed). Properties `tag`, `type`, free `maker`, `length`,
  `designation`.
- **024 reports:** the cable report's `Cores` is the member count and `Used` and `Spare` show `—`
  "until brief 028"; the BOM counts a terminal strip once (024 Q11). Generated pages: cover,
  contents, revisions, BOM by location.
- **025** reference index: terminals are class `Other`, so they have no references.
- **026** (merging): one planner for device tags, cable tags (`-W01`) and wire numbers, in drawing
  order; terminal labels are not numbered.

## Scope

1. **Placing terminals.** Place a terminal on a wire: the wire is split there and both halves land
   on the terminal, in one transaction. **Place Terminals** puts one terminal on each wire a fence
   line crosses, numbered in order. A PE terminal starter. Spare terminals through a strip's
   `terminals` property.
2. **Bridges.** A wire between two terminals of one strip is a bridge: derived, no new data. Draw
   Bridge makes one between adjacent terminals.
3. **Terminal numbering** through 026's planner: a fourth kind, `terminals`, per strip in drawing
   order.
4. **Cores.** A cable's `cores` property (or its type, `7G1.5`) says what it offers. Used, spare
   and over are derived; a new member takes the next free core.
5. **Terminal plan** per strip and **cable schedule** per cable, derived, shown in Reports and on
   generated pages, readable by MCP. Each ships an update map.
6. **Fit with 024 and 025:** the BOM counts terminals per strip and type; the wire report marks
   bridges; the cable report fills Used and Spare; plan rows use 025's place cache.
7. **Checks** rows, each with a way out. **MCP** for every command. **Bench** rows.

Out of scope: multi-level terminals (design Q4), disconnect and fuse terminals, terminal
accessories (end plates, markers), the cable-type list and CSV export (027), cable lengths from the
drawing, core colours by standard tables beyond `G`.

## Format and ops

No new op is proposed for the main path. The overseer decides FP1 to FP5 (design §13): strip
property `terminals`, cable property `cores`, page property `strip` on a `terminals` generated
page, BOM part keys per terminal type, and (only if a worker finds Remove/Add of one terminal ID in
one transaction is refused) a `RelabelTerminal` op.

## Acceptance criteria

1. Fixture T (design §12): placing a terminal on a wire splits it into two conductors on one
   net with one wire number; `u` restores the single wire. Place Terminals across four wires adds
   `-X01:1…4` left to right in one transaction.
2. A wire between `-X01:3` and `-X01:4` is listed as a bridge in the plan and as `Bridge` in the
   wire report's Cable column. Draw Bridge across adjacent terminals is one transaction.
3. `renumber {kind: "terminals"}` numbers each strip's terminals 1…n in drawing order; a preview
   lists the rows; one transaction, one `u`; wires stay attached.
4. `-W01` of type `7G1.5` with five members: Cores 7, Used 5, Spare 2 (`6, GNYE`) in the cable
   report and schedule. An eighth member gives a `too-many-cores` Checks row; nothing is refused.
5. The terminal plan of `-X01` lists every terminal (drawn and spare) with inside and outside
   targets, wire numbers, cable and core, bridges and place, on the Reports tab and on a
   `terminals` generated page. Moving a field device to another location flips the sides only of
   its terminals' rows.
6. BOM: `-X01` with 10 feed-through and 2 PE terminals gives two rows, Qty 10 and 2.
7. Every Checks row of design §8 has a working fix or go-to.
8. MCP contract tests (brief 011 rule): `place` on a wire, `terminals`, `bridge`, `renumber
   {kind: "terminals"}`, `setprop` `terminals` and `cores` record `Author::Agent(<client>)` and are
   refused with writes off; `stripplan`, `cablecores`, `cablelist` and `checks` work with writes
   off and record nothing.
9. Bench rows of design §11 within their targets; no ordinary edit rebuilds a plan or schedule.
10. Save and reopen keep everything; an old file opens unchanged and its strips show a plan.
11. Reference screenshots, approved by the owner.

## Needs owner check

- Approve the design (`docs/design/028.md`) and answer its questions (§14).
- A click-only trial: quick start 5M.
