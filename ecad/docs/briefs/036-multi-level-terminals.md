# Brief 036: Multi-level terminals

Status: Approved (owner, 2026-11-13: design 036 §15 Q1–Q12 answered "as recommended"; FP1–FP8
approved by the overseer, 2026-11-12, design §14; decisions in design §17)
Branch: `feature/multi-level-terminals`, from `origin/dev` `1e605e33`.
Designer: **yes** (`docs/design/036.md`) · Size: M (the roadmap's estimate; the design's §0 file
list makes it a large M)
Roadmap: `docs/discussions/roadmap-review-2026-11.md` M1 item **1.2**, lane D, first. Decided there:
owner question 5 (a link without a level means level 1). Item 1.8 (graphical strip pages) reads
this brief's strip data and starts after it lands; 2.6 (terminal manager) replaces Reports ▸
Terminals later on the same row model.

## Goal

Draw and list a terminal strip whose positions have several levels: L/N/PE installation terminals,
3-level sensor terminals, stacked and double-deck terminals, and fuse and disconnect terminals
that break the net. Each level has its own label (`1.L`, `2.1S`), its own links, and can be drawn
on the page where its circuit is. The terminal plan, the BOM and renumbering treat a position as
one part with several levels.

## What exists today (`origin/dev` `1e605e33`)

- A strip (`-X01`) is one device; each terminal is one symbol with one connection point (028).
  Its label is a whole number from joining or renumbering; there is no way to relabel it, so
  `1.L` cannot be written. A joined terminal's `key` is its first label, not a definition key.
- The strip's `terminals` offer (`12+2PE`) adds spare positions; the BOM gives one row per strip
  per terminal type (feed-through, PE).
- Links (`link.<n>`, 030) join whole positions by label; 022's connectivity reads them as a
  bridge source; the Jumper tool makes them.
- `Definition::bridges` (022 FP5) join keys of a device's own definition across the **whole
  device**, so on a strip they cannot say "level 1 and 2 of one position".
- 034 adds `fuseterm`, a two-point fuse device (`-F`), which breaks the net; its question 1 left
  "turning it into a strip position" to this brief.
- The defaults registry (031 §3.5) holds every new default.

## Scope

1. **Levels per position.** A position is the strip terminals that share one position label; each
   is one level (one connection point, or two for fuse and disconnect levels). A terminal **type**
   (feed-through, PE, 2-level, 3-level, L/N/PE, sensor, disconnect, fuse, 2-level bridged) says
   which levels a position has. Types are catalogue code; level names are registry defaults.
2. **Drawing a level where its circuit is.** One starter symbol per type and level, plus one
   whole-terminal symbol per multi-level type. Placing a level fills the next position of that type
   that lacks it, so drawing the L row then the N row fills positions 1…n.
3. **Labels per level, and relabelling.** The shown label is the position, a separator and the
   level name (`1.L`), or a per-level override. `termlabel` relabels a position (all its levels)
   or sets one level's text. Renumber keeps working per position.
4. **Links per level.** `link.<n>` values name levels (`1.2-8.2`, `1.1-1.3`); a position without
   a level means level 1 (roadmap question 5). The Jumper tool links one level, or the levels of
   one position.
5. **Internal bridges** from the type (a bridged 2-level terminal joins its levels inside).
6. **Fuse and disconnect positions** break the net. Their fuse rating is a strip property. 034's
   fuse terminal devices can be turned into strip positions by the user (`toposition`), offered
   from Checks, never automatic.
7. **Terminal plan, BOM, numbering:** plan rows per level, grouped by position, mirrored inside |
   strip | outside; spare positions by type from the offer (`12+2PE+6x3`); one BOM row per strip
   per type with Qty in positions; renumber through 026's planner per position.
8. **Row model for the terminal manager (2.6) and strip pages (1.8):** every plan field is named as
   editable (with the one command that writes it) or derived.
9. **Defaults** through the registry: level names, the level separator, type descriptions.
10. **Update maps** for every derived row; **Checks** with a way out; **MCP** for every command;
    **bench** rows.

## Later (not designed now)

- Graphical strip pages (1.8) and the editable terminal manager (2.6): this brief fixes their data.
- Fuse inserts as BOM rows (1.12); test plugs and level markers printed (3.3).
- Turning a strip fuse position back into a device; a per-position type override.

## Format and ops

No new op and no new record field. FP1–FP8 (design §14), **approved by the overseer
(2026-11-12)**, including FP8's `ecad-model` changes and their update scope: the offer grammar
gains type codes; level terminals use definition keys `.n` (`.ni`/`.no` for two-point levels); link
values gain levels; strip keys `term_label.<pos>.<n>` and `rating.<pos>`; BOM part keys per type;
renumber and `termlabel` move these keys in their transaction; three `ecad-model` code changes
(connectivity reads level bridges per position, links match levels, the numbering planner numbers
positions the caller groups). The registry gains `defaults/terminals.rs`.

## Acceptance criteria

The full table is design §13. In short:

1. Fixture M (design §13): drawing L, N and PE levels with Place Terminals across three wire rows
   gives positions 1…3 with labels `1.L`…`3.PE`, three positions on the BOM, one transaction per
   fence; `u` undoes each.
2. A level drawn on another page joins its position; the plan lists both pages' places; renumber
   numbers positions in drawing order and moves `term_label.*`, `rating.*`, `acc.*` and `link.*`.
3. `link.1 = 1-3` on an L/N/PE strip joins level L only; `1.2-3.2` joins N; `1.1-1.3` joins one
   position's levels; old 030 links read the same.
4. A bridged 2-level position joins its two levels into one net; a fuse or disconnect position's
   two points are two nets with two wire numbers.
5. `toposition` turns a `fuseterm` device into a fuse position of a chosen strip in one
   transaction, wires attached, rating carried; `u` restores the device.
6. Offer `12+2PE+4x3`: the plan shows spare positions with their levels; the BOM shows three rows
   per strip, Qty in positions.
7. `termlabel` relabels a position and a single level; the page, plan, hover and MCP show it.
8. Every Checks row of design §8 has a working fix or go-to.
9. MCP contract tests (brief 011 rule) for every new or changed command.
10. Changing a registry default (separator, level names) recomputes only the strips that inherit
    it; verify-mode equality; bench rows within target.
11. Save and reopen keep everything; a 030 file opens and reads the same.

## Needs owner check

- Reference screenshots (design §13) and a click-only trial (quick start 5P).
