# Brief 044: Connector devices

Status: Draft (designer, 2026-11-13). Waits for the owner's answers (design §14) and the
overseer's ruling on the format items (design §13).
Branch: `feature/connectors`, from `origin/dev` `be6f4413`.
Designer: **yes** (`docs/design/044.md`) · Size: M (the roadmap's estimate; the design keeps it
there by reusing 036's per-position bridges and 041's Place group)
Roadmap: `docs/discussions/roadmap-review-2026-11.md` M1 item **1.11**, lane **S**, first in M1
(before 1.1 symbol builder). Builds on 034 (the connector bodies), 036 (position and level keys,
per-position bridges), 025 (the reference index), 024 (BOM and locations), 031 (the defaults
registry), 041 (the Place group and the intent line) and 042 (the inspector). Owner answer Q11
(UI spec): halves are named `-X10.S` and `-X10.P`, as a Standards setting.

## Goal

A plug and its socket are drawn as one connector that mates: each pin is a connection point on
each half, the two halves of a pin are one net, the halves and single pins can sit on different
pages, each half is listed on the BOM with its own part and location, and each half shows where
its mate is.

## What exists today (`origin/dev` `be6f4413`)

- 034 built the bodies: `socketN`, `plugN` (2/3/4/6/8/12), D-sub 9/15/25 and M12 4/5/8 halves,
  and `rj45`, `usba`, `usbb` bulkheads, all `Kind::Fixed { exclusive: true }`, prefix `X`, every
  pin quiet. **Each half is a device of its own** (034 question 2): a socket and its plug are two
  tags, two nets per pin and no relation between them.
- 036 gave strip terminals level keys (`.1`, `.2`) and made `Definition::bridges` join keys **per
  label** for `.`-keys (FP8a). That rule is exactly "pin 3 of this half joins pin 3 of that half".
- The Place group (041) has Symbol and Terminal; Connector is listed in the spec but has no
  command yet, so it is not shown.
- The drawing-pack redraw drew every connector as two loose halves or as plain boxes; its
  terminal-strip pages name a half in every target (a half tag and a pin), which ECAD cannot write.

## Scope

1. **One connector device, two halves.** A connector (`-X10`) is one device. Its socket half and
   plug half are its two **sides**; each pin of each half is one connection point (terminal key
   `.s` or `.p`, label = the pin name). The halves and single pins can be drawn on any pages.
2. **Mating.** Pin `n` of the socket half and pin `n` of the plug half are one net, through 036's
   per-position bridges. Nothing else joins.
3. **Naming per Q11.** A half shows and is typed as `-X10.S` / `-X10.P`: the tag, a separator and
   a side name, all registry defaults (a project can write `-X10.F` / `-X10.M`). Pin names are
   numbers, letters or a typed list (`L,N,PE`), shared by both halves; `pinlabel` renames a pin.
4. **Placement through Place ▸ Connector** (041's action table, no new panel): one command,
   `connector`, whose prompts ask which halves, the type, the pin count, the pin names and
   whether to draw the halves as rows or pin by pin across wires. Find Symbol keeps placing a
   single half or pin, joining a connector when one is named.
5. **Mating rules and checks:** one half per side, a pin once per side, same family and size on
   both halves, a lone pin's mate missing. Refusals name a way forward; checks have a go-to and a
   fix where one exists.
6. **BOM:** one row per drawn half, tag `-X10.S`, with the half's own description, maker, part
   number and location.
7. **Cross-references** between the halves, and between a pin and its mate on another page,
   on 025's reference index: live, with hover and go-to.
8. **Defaults** through the registry (new area `connector`); **update maps** for every derived
   view; **MCP** contract tests for every new command; **bench** rows.

## Later (not designed now)

- Assembly rows for crimp contacts, housings, backshells and covers per half (1.12, brief 047:
  design §8.3 proposes how they name a half).
- A connector named under the device it belongs to (as the pack does): the 81346 aspects work.
- Cordsets (a moulded plug on a cable) and patch cords into bulkheads: the cable manager (2.7).
- A connector plan report (pins, both halves' targets): the report generator (2.9).
- Merging two one-half connectors into one (`mate`), for drawings made before 044.

## Format and ops

No new op and no new record field. Design §13 lists FP1–FP6 for the overseer: pin keys `.s`/`.p`
(existing `Terminal.key`); mating through 036's FP8a with `bridges` on the half definitions;
version 2 of the 24 mating half definitions (`library_seq`) and the lookup by content hash; the
component properties `pins` and `half.<s|p>.<field>`; the registry area `connector`; and a guard
in lane D's level readers so a connector pin is never read as a strip level.

## Acceptance criteria

The full table is design §12. In short:

1. `connector {halves: both, family: plain, pins: 4, layout: row, at}` gives `-X10` with
   `-X10.S:1…4` and `-X10.P:1…4`, one transaction, one `EmbedDefinition` per new half definition;
   `u` undoes it.
2. A wire to `-X10.S:3` and a wire to `-X10.P:3` are one net and carry one wire number; pins 3 and
   4 are two nets. This holds with the halves on different pages and with pins drawn one by one.
3. A fence across four wires with `layout: pins` places four mated pin pairs on the next free
   pins; each wire is split, the socket side keeps its number and the plug side is the same net.
4. A second socket half, a pin drawn twice on one side, and a half joined to a non-connector are
   refused with a way forward; a family or size mismatch and a lone pin give the Checks rows of
   design §6.3.
5. The page, hover, wire list, terminal plan and MCP name a pin `-X10.S:3`; typing `-X10.P:3`
   finds it; a project side name `F` shows `-X10.F` everywhere at once.
6. The BOM has one row per drawn half with that half's description, maker, part and location;
   a lone half gives one row; a bulkhead one row, as today.
7. Each half's tag shows its mate's place (`/102.C4`); a pin whose mate is on another page shows
   the mate's place; hover and Go to Reference reach it.
8. Every registry default (separator, side names, half descriptions, the maker's first answers)
   changes what new and existing connectors show, recomputing only the connectors that inherit it.
9. MCP contract tests for `connector` and `pinlabel`, and for joining a half through `place`.
10. Hostile input never panics: 10 000 pins, a 10 000-character pin list, a pin name with control
    characters, `half.<x>.part_number` with an unknown side.
11. Bench rows within target (design §11.3); verify-mode equality for the connector index.

## Tester

**Yes.** The lane adds a command with prompts, joins nets across pages through a bridge rule,
changes BOM rows and what wire lists and strip pages name, and versions starter definitions: each
needs checking from the outside (design §12), not only by the worker's tests.

## Needs owner check

The owner questions in design §14, and a click-only trial (quick start `044-connectors.md`):
draw a sensor with a mated M12 pair pin by pin across its three wires, and a D-sub pair as rows
on two pages.
