# Brief 057: wire and cable rating checks

Status: Draft
Branch: `feature/ratings`, from `origin/dev` `8b6c2ccd`.
Designer: **yes** (`docs/design/057.md`) · Size: L (split below)
Release: **0.2.2** (UI rev 4). The Power brief (0.2.3) supplies load currents and the panel's
computed temperature through the interfaces this brief defines.
Source: the rev 4 build spec (14 November 2026), "Wires, with conductor rating", "Cables, with IEC
rating" and "Calculations: where each one lives"; boards 2.05 and 2.06.

## Goal

Every panel wire and field cable that has rating data is checked against the device that protects
it: **Ib ≤ In ≤ Iz**. Panel wires are rated from the part's datasheet (rating at its reference
temperature × panel temperature factor × bundling factor), field cables from IEC 60364-5-52 /
BS 7671 tables (method, table rating for the size, ambient and grouping factors), each with a
Manual override. Field cables also get a volt drop against a limit (5 %). Results are rows in the
Checks list, with the bottom counter, a go-to and a fix. Calculations are checks, not tools.

## What exists today (`dev` `8b6c2ccd`)

- **022:** potentials, conductor `colour` and `cross_section`, the Checks list and counter.
- **028 / 032:** cables with `type` (notation `4G1.5`, read by `strips::type_notation`), `length`
  and core offers; `CableRow`.
- **034 / 036:** the device `rating` property (free text, `13A C 30mA`), `rating.<n>` on fuse banks
  and on strip fuse positions (`symbols::setprop_rule`).
- **050:** the style model; wire categories with cross-section, part number and maker; the
  catalogue word `cable` reserved for cable types (FP7); registry `AREAS`.
- **Rev 3:** Project ▸ Wires and Cables tabs with a right panel.
- **Not there:** a parts catalogue (2.3), a cable manager (2.7), a load tree (0.2.3).

## §0 Files touched

Design 057 §0 is the full table.

- **Owned:** `ecad-cadcraft/src/rating/` (`mod.rs`, `tables.rs`, `trace.rs`, `load.rs`,
  `parse.rs`), `rating_cmds.rs`, `styles/cable.rs`, `defaults/rating.rs`,
  `docs/quick-start/057-ratings.md`, and the `lane_057.rs` test modules.
- **Shared, by hunk:** `styles/wire.rs` (five fields), `styles/mod.rs` and `defaults/styles.rs`
  (one line each), `defaults/mod.rs` (one `AREAS` line), `checks.rs` (kinds, `Key::Rating`, fixes),
  `strips.rs` (`notation_size`), `lanes.rs` (one line), the trial's `lists/wires.rs`,
  `lists/cables.rs` and `settings/standards.rs` (one `CATALOGUES` line and the Rating defaults).
- **Not touched:** `ecad-model`, `ecad-file` (no format change beyond properties and style
  fields).

## Scope, in order

1. **057a, the calculation core** (no UI): parsing with bounds (design §6.1), the three bases and
   volt drop (§3), rounding (§3.6), the device rating reader and the trace (§4.1–§4.2),
   `LoadSource` with `TypedLoads` (§4.3), the wire-category fields and the `cable` catalogue (§4.4),
   the `rating` registry area (§6.3), the built-in table **structure** with fixtures and the
   self-check tests (§5).
2. **057a, checks and commands:** the check kinds of §7 with their update map (§10), `rating` and
   `ratings` (§8), MCP contract tests.
3. **057b, the screens:** the Rating sections of Project ▸ Wires and Cables as boards 2.05 and 2.06
   show them (§2), Status / Checks columns, Standards ▸ Cable types and Defaults ▸ Rating.
4. **057c, the table values** (not the worker's): a person with licensed copies transcribes the
   first set (§5.1), a second checks it (§5.3). Can land after 057a/b; until then results show
   "unverified".
5. Bench rows (§10) and the quick start.

## Out of scope

Fault protection (k²S², Zs, disconnection times). Volt drop from the origin, diversity and peak
(Power, 0.2.3). Aluminium; methods other than B1, B2, C, E; sizes outside 1.5–16 mm² in the tables;
flexible-cord tables. Ctrl K quick sums. The parts catalogue (2.3), cable manager (2.7) and panel
layout (rev 4). Any calculator screen.

## Format and ops

Design §13, FP1–FP7. No op, no `Document` field, no type change: new conductor, cable and
location-box properties, five wire-category fields, the `cable` catalogue (050 FP7's reserved word)
with choice entry `cable_type`, and registry area `rating`. Old files open unchanged. **For the
overseer.**

## Update map

Design §10. A protective device's rating change re-checks only the subjects it protects; a
connectivity change re-traces only subjects whose trace path crosses a touched net; a category or
cable-type change re-checks its users; a location's temperature re-checks the wires at it; a
`rating` default re-computes results but not traces. Every other op does nothing. Tables patch the
re-checked rows' cells only.

## Acceptance criteria

1. Design §11 checks 1–19 pass, including the worked examples 1–9 with the exact numbers given.
2. A project with no rating data shows no rating rows (no nagging).
3. Every table value in `tables.rs` cites standard, edition, table and column; the self-checks of
   §5.3 pass; `rating-unverified` shows while any used table is unchecked.
4. MCP contract tests for `rating`; `ratings` works with writes off.
5. Update-map counters on the medium project: a device rating change re-checks only its
   subjects, no table rebuild.
6. Boards 2.05 and 2.06 compared with screenshots of the same states.
7. `docs/quick-start/057-ratings.md` as design §12.

## Tester

**Yes.** These are safety-relevant calculations. The tester runs design §11 in full, recomputes
every worked example by hand from the fixtures and the formulas of §3 (not from the code's output),
tries the hostile values of §6.1 and checks 10 and 11, and runs the fast tests of `ecad-cadcraft`
and `ecad-trial`.

## Owner questions

Design 057 §14, questions 1–17. The ones that block 057a: 1 (cable types as a catalogue), 2 (the
trace), 3 (In as the volt-drop current), 6 (temperature cap), 7 (bundling unset) and 11 (table
licensing and who verifies).

## Needs owner check

Name the two people who transcribe and check the IEC tables (Q11), and confirm the board 2.06
example (19.2 A, not 19.3, Q16).
