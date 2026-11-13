# Brief 045a: PLC rack, slot and card model

Status: Draft (designer, 2026-11-13)
Branch: `feature/plc`, from `origin/dev` `be6f4413`. 045a, 045b and 045c land on it in order.
Designer: **yes** (`docs/design/045.md`, §2, §3, §9 a-rows, §11, §12) · Size: M
Roadmap: `docs/discussions/roadmap-review-2026-11.md` M1 item **1.3(a)**, lane D, after 1.2 (036).
The PLC manager (2.8) later draws this model as a column set on 039's table; no panel here.

## Goal

A CPU or head module is a rack with numbered slots, and each card (DI, DQ, AI, AO, RTD, IO-Link,
power, communication) is a device in one slot, so the PLC is drawn and listed as it is built.

## What exists today (`origin/dev` `be6f4413`)

- 034's card bodies `plccpu`, `plcps`, `plcdi`, `plcdq`, `plcai`, `plcao`, `plcrtd`, `plciol` are
  heads (prefix `A`) with supply points; nothing relates them to each other.
- No property refers to another component; IDs have a 32-hex text form (`ComponentId::hex`).
- The defaults registry (031 §3.5) with one file per lane; 036 added the `terminal` area.

## §0 Files touched

| File | Change |
| --- | --- |
| `ecad/crates/ecad-cadcraft/src/plc/mod.rs` (**new**) | `specs()`; the kept `Plc` model, its update from the touched set (design §11.1) |
| `ecad/crates/ecad-cadcraft/src/plc/rack.rs` (**new**) | Card types (design §3.1), racks, slots, `plcslot`, `plcracks` |
| `ecad/crates/ecad-cadcraft/src/defaults/plc.rs` (**new**) | Design §12's a-entries; `defaults/mod.rs`: `plc` in `AREAS`, the `mod` line, one link in `all()` |
| `ecad/crates/ecad-cadcraft/src/lib.rs` | `mod plc;` |
| `ecad/crates/ecad-cadcraft/src/lanes.rs` | `("045", crate::plc::specs),` |
| `ecad/crates/ecad-cadcraft/src/symbols.rs` (lane S; append-only, the overseer tells lane S) | `plcim`, `plcpm`, `plccm` bodies (`pc(9)`–`pc(11)`); the `Caption` anchor on card bodies |
| `ecad/crates/ecad-cadcraft/src/device_cmds.rs` | Rack/Slot prompt when placing a card body; deleting a rack takes its cards out |
| `ecad/crates/ecad-cadcraft/src/checks.rs` | One block: `slot-twice`, `rack-missing`, `slot-range`, `unracked` |
| `ecad/crates/ecad-cadcraft/src/interact/forms.rs`, `actions.rs` | Properties ▸ Card and ▸ Rack fields; wheel row **Slot…** |
| `ecad/crates/ecad-cadcraft/tests/all/lanes/lane_045.rs`, `lane_045/rack.rs`; one `mod lane_045;` in `tests/all/lanes/mod.rs` | Contract tests |
| `ecad/apps/ecad-mcp/tests/all/lanes/lane_045.rs`; one `mod` line | MCP contract tests |
| `ecad/crates/ecad-bench` | `plcbuild` row (design §11.2) |
| `ecad/docs/quick-start/045-plc.md` (**new**) | Section 1 |
| `ecad/docs/glossary.md` (append) | Design §1's words |

Not touched: `ecad-model`, `ecad-store`, `ecad-file`, upstream, CI.

## Scope

1. Racks (`plccpu`, `plcim`) and cards (every other card body) as devices; card types from the
   catalogue (design §3.1).
2. Card properties `plc.rack` (the rack's ID) and `plc.slot`; the rack's slot range from the
   registry; channel counts per type from the registry and per card.
3. Placing a card body asks Rack and Slot; `plcslot` moves, swaps or takes out; `plcracks` reads.
4. Deleting a rack takes its cards out in the same transaction.
5. The card caption (`{rack} slot {slot}`) as a registry pattern.
6. Checks with fixes; inspector fields; wheel row; update map; MCP; bench.

## Acceptance criteria

Design §13 rows A–E, M (a-rows), R (a-commands), S (`plcbuild`), T (a-inputs). In short:

1. Placing six card bodies after a `plccpu` offers the rack and the next free slot; `plcracks`
   lists them in slot order; one transaction each.
2. `plcslot` refuses a taken slot with the way out, swaps with `swap: true`, takes out with
   `rack: null`; `u` undoes each.
3. `slot-twice`, `rack-missing`, `slot-range` and `unracked` appear for their cases and every fix
   works.
4. Save and reopen keep everything; retagging the rack changes no card property and every caption
   follows.
5. A file with 034 card bodies opens; they are cards in no rack.
6. Deleting a rack leaves its cards in no rack in one transaction; `u` restores both.
7. MCP contract tests (brief 011 rule): `plcslot`, `place` of a card body with rack and slot,
   `setprop plc.*`, `setdefault plc.*` record `Author::Agent(<client>)` and are refused with writes
   off; `plcracks`, `checks`, `defaults {group: "PLC"}` work with writes off.
8. The `Plc` model's update map (design §11.1): an op not listed recomputes nothing (counter);
   verify-mode equality; `plcbuild` within target.
9. Hostile inputs of design §13 T for slots, racks and captions: a refusal in words or a Checks row,
   never a panic.
10. FP1's conditions (overseer, 2026-11-13): merge, diff and the inspector show the rack's tag,
    never the hex ID; deleting a rack clears the keys on its cards in the same transaction (6); a
    dangling ID is a `rack-missing` Checks row, never a crash (3); a round-trip test (save, reopen,
    compare) covers `plc.rack`.

## FP items (overseer)

FP1 (`plc.rack` holds a component ID: the first property that refers to a component), FP4
(registry keys as object properties), FP7's `plc` registry area. No op, no record field.
**Approved (overseer, 2026-11-13)**, FP1 on the conditions in criterion 10; the `plc` area joins
047 FP6's one-line-per-area `all()` list, made by whichever of 045 and 047 lands first. Decisions in
design §14.

## Tester

Yes: FP1 is a new kind of property reference, and placement and delete gain ops.

## Needs owner check

Design §15 questions 1, 2, 15, 17. Reference screenshot `045-rack-props.png`.
