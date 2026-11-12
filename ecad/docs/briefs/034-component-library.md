# Brief 034: Component library build-up

Status: Approved (owner, 2026-11-12: all eight questions in design §14 answered "as recommended";
design `docs/design/034.md` revision 2; no format change, confirmed by the overseer)
Branch: `feature/component-library`, from `origin/dev` `6729c8f1`.
Designer: **yes** (`docs/design/034.md`) · Size: L, in four stages
Roadmap: `docs/discussions/roadmap-review-2026-11.md` §3.2 v0.15 **Q9**, lane **S**. Owner decision
2026-11-12 (question 20): built-in components before the symbol builder (M1 1.1). Later items that
build on it: 1.1 symbol builder, 1.3 PLC racks and cards (uses the card bodies), 1.11 connector
devices (adds mating to the connector bodies).

## Goal

The symbols a real control-panel pack needs are built in, so nobody draws a black box by hand: each
is our own drawing in code, with sensible connection points, a tag prefix, a default description,
text anchors for lane T to place text at, and one BOM row per device.

## What exists today (`dev`, `6729c8f1`)

- 21 starter symbols in `ecad-cadcraft/src/symbols.rs` (018, 022, 028): coil, NO and NC contacts,
  3-pole power contacts, MCB 1- and 2-pole, fuse, PSU, push buttons, selector, lamp, motors,
  transformer, terminal, PE terminal, PLC DI and DO channels, junction, continuation.
- Each is a `Definition` with a fixed origin (`STARTER_LIBRARY`, a fixed `DefinitionId`), embedded
  into a project by `EmbedDefinition` on first use and found again by origin. Kind, prefix, aliases
  and description live in the catalogue, never in the file.
- A symbol makes a new device or joins one (design 018 §3.1). Contacts take the IEC sequence digit
  (`13`/`14`, `21`/`22`); fixed symbols bring their definition's labels.
- `Definition.bridges` (022 FP5) exists; no starter uses it.
- No change-over contact, 4-pole contactor, RCBO, RCD, SSR, fuse bank, field sensors, valves,
  thermocouples, heaters, connectors or PLC card bodies. The drawing-pack redraw drew 39 of these
  as plain boxes.

## Scope

Built-in definitions, in four stages in the order of the pack's ranking (design §3–§7):

1. **Contacts and protection:** change-over contacts (`11`/`12`/`14`) on relays and switches;
   4-pole power contacts; RCBO and RCDs; SSR input and output; fuse terminals with ratings; fuse
   banks (one device, one connection point per fuse, a rating per fuse).
2. **Sensors and field devices:** proximity switch, float and roller switches, interlock
   switches, single and dual solenoid valves, thermocouple, heater.
3. **Connector and card bodies:** plug and socket halves, D-sub, M12 fieldbus, RJ45 and USB
   bulkheads; PLC card bodies (CPU, power, DI, DQ, AI, AO, RTD, IO-Link) that 1.3 will use.
4. **Contact tables and offers** for change-over contacts, 4-pole contacts and SSR outputs, after
   lane P's brief 033 has merged into `dev` (design §8, question 7).

Each family has its IEC 60617 basis (the default, never the law), its connection-point names, text
anchors only (lane T, brief 031, decides text sizes and placement), and its BOM row (design §9).
A `rating` property (and `rating.<n>` per fuse of a bank) holds ratings. The defaults this lane
adds (prefixes, labels, quiet points, ratings) are registered in lane T's defaults registry (031
§3), not hard-coded (design §2.7).

## Out of scope

- Mating plug and socket halves as one device (M1 1.11); channels, slots and addressing on PLC
  cards (M1 1.3); fuse terminals as multi-level strip positions that break the net (M1 1.2).
- Text sizes, fonts, placement and visibility (lane T, 031); contact-table layout (lane P, 033).
- A symbol builder, user symbols, `.ecadlib` and a parts catalogue (1.1, 2.1, 2.3).
- New commands. Every symbol is placed with the existing `place {symbol}`; nothing is registered in
  `commands.rs`.

## Acceptance criteria

1. Every new symbol in design §3–§7 is in the catalogue with the key, prefix, connection points and
   geometry the design gives; its connection points are on the 5 mm grid; its content hash is its
   origin's; its definition ID is unique and never reused.
2. Placing each new symbol from Find Symbol and through `place {symbol}` makes a device with the
   family's prefix and the next number, the design's labels and the default description; the
   definition is embedded once and reused by origin.
3. A change-over contact joining a device takes the next sequence digit: `-K01` coil + `co` →
   `11`/`12`/`14`; + `no` → `23`/`24`; + `co` → `31`/`32`/`34`. After the ninth contact it is
   refused with `-K01 already has 9 contacts.`
4. `rating` is set with `setprop` on any device; `rating.<n>` only on a fuse bank, where `<n>` is
   one of its fuse outputs (`-F01 has no fuse 13.` otherwise); values follow the 018 §6.1 rules.
5. Each family's BOM row is as design §9 says, with no new BOM column.
6. `symbols::anchors` gives every starter symbol (old and new) its text anchors (design §10); lane
   T reads them through that one function.
7. **MCP contract test** (`ecad/apps/ecad-mcp/tests/all/library_contracts.rs`, design §12):
   placing each new symbol through `execute` records one transaction authored
   `Author::Agent(<client name>)` with the expected embed, device, labels and description; a
   second placement reuses the embedded definition; a change-over join and a rating are recorded
   the same way; every one is refused with writes off and the head is unchanged.
8. Stage 4 (after 033 is in `dev`): change-over contacts appear in contact tables and offers
   (`1CO`), 4-pole contacts and SSR outputs as power poles (design §8).
9. Hostile input never panics: unknown keys, oversized or control-character ratings, `rating.<n>`
   with a huge or non-numeric `<n>`.
10. `docs/glossary.md` and `docs/quick-start.md` are updated for the new count and families.

## Stages and lane rules

- The worker lands stages 1, 2 and 3 in order, each as commits that build and pass the scoped
  checks; stage 4 waits for 033 in `dev` and merges `dev` first.
- Lane S owns `symbols.rs` and `convert.rs`. Edits outside them are listed in design §0, each
  one small and append-only; the overseer tells lanes T and P before the worker makes them.
- Tests: the scoped tests of `ecad-cadcraft` and `ecad-mcp` only; no slow set.

11. Every default in design §2.7 is read through lane T's defaults registry; a test changes one
    registered default (a prefix) and a newly placed device follows it.

## Owner questions

Design §14: all eight answered "as recommended" (owner, 2026-11-12).
