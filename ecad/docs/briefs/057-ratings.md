# Brief 057: wire and cable rating checks

Status: Approved (owner, 2026-11-14: design §14 answered, Q11 user-entered tables only, Q17 IEC
method names and factors, all others as recommended; overseer, 2026-11-14: FP1–FP7, FP8 from Q11).
Branch: `feature/ratings`, from `origin/dev` `8b6c2ccd`.
Designer: **yes** (`docs/design/057.md`) · Size: L (split below)
Release: **0.2.2** (UI rev 4). The Power brief (0.2.3) supplies load currents and the panel's
computed temperature through the interfaces this brief defines.
Source: the rev 4 build spec (14 November 2026), "Wires, with conductor rating", "Cables, with IEC
rating" and "Calculations: where each one lives"; boards 2.05 and 2.06.

## Goal

Every panel wire and field cable that has rating data is checked against the device that protects
it: **Ib ≤ In ≤ Iz**. Panel wires are rated from the part's datasheet (rating at its reference
temperature × panel temperature factor × bundling factor), field cables from a **rating table the
user enters or imports** (IEC 60364-5-52 method, the table's rating for the size, ambient and
grouping factors), each with a Manual override. **ECAD ships no IEC or BS 7671 table values**
(owner, Q11): the repository is public and the numbers are the standards' content. Field cables also get a volt drop against a limit (5 %). Results are rows in the
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

- **Owned:** `ecad-cadcraft/src/rating/` (`mod.rs`, `dataset.rs`, `trace.rs`, `load.rs`,
  `parse.rs`), `rating_cmds.rs`, `styles/cable.rs`, `styles/ratingtable.rs`, `defaults/rating.rs`,
  `docs/quick-start/057-ratings.md`, and the `lane_057.rs` test modules.
- **Shared, by hunk:** `styles/wire.rs` (five fields), `styles/mod.rs` and `defaults/styles.rs`
  (one line each), `defaults/mod.rs` (one `AREAS` line), `checks.rs` (kinds, `Key::Rating`, fixes),
  `strips.rs` (`notation_size`), `lanes.rs` (one line), the trial's `lists/wires.rs`,
  `lists/cables.rs` and `settings/standards.rs` (two `CATALOGUES` lines, Cable types and Rating
  tables, and the Rating defaults).
- **Not touched:** `ecad-model`, `ecad-file` (no format change beyond properties and style
  fields).

## Scope, in order

1. **057a, the calculation core** (no UI): parsing with bounds (design §6.1), the three bases and
   volt drop (§3), rounding (§3.6), the device rating reader and the trace (§4.1–§4.2),
   `LoadSource` with `TypedLoads` (§4.3), the wire-category fields and the `cable` catalogue (§4.4),
   the `rating` registry area (§6.3), the `ratingtable` catalogue with its CSV parser, bounds,
   lookups and plausibility checks (§5). No table values anywhere outside `tests/`, and there only
   synthetic ones.
2. **057a, checks and commands:** the check kinds of §7 with their update map (§10), `rating`,
   `ratingtable` and `ratings` (§8), MCP contract tests.
3. **057b, the screens:** the Rating sections of Project ▸ Wires and Cables as boards 2.05 and 2.06
   show them (§2), Status / Checks columns, Standards ▸ Cable types, Standards ▸ Rating tables
   (rows, Import CSV…, Export CSV) and Defaults ▸ Rating.
4. Bench rows (§10) and the quick start.

## Out of scope

Fault protection (k²S², Zs, disconnection times). Volt drop from the origin, diversity and peak
(Power, 0.2.3). Aluminium. Any table values shipped with ECAD (a 0.4.0 standards review decides).
Volt drop above 16 mm² (reactance). Ctrl K quick sums. The parts catalogue (2.3), cable manager (2.7) and panel
layout (rev 4). Any calculator screen.

## Format and ops

Design §13, FP1–FP8. No op, no `Document` field, no type change: new conductor, cable and
location-box properties, five wire-category fields, the `cable` catalogue (050 FP7's reserved word)
with choice entry `cable_type`, the `ratingtable` catalogue (FP8) chosen by `rating.table`, and
registry area `rating`. Old files open unchanged. **Approved by the overseer (2026-11-14):
FP1–FP7.** FP8 was added after the owner's Q11 answer and uses FP4's mechanism (a catalogue word,
no op).

## Update map

Design §10. A protective device's rating change re-checks only the subjects it protects; a
connectivity change re-traces only subjects whose trace path crosses a touched net; a category or
cable-type change re-checks its users; a rating-table change re-computes Table-basis results; a location's temperature re-checks the wires at it; a
`rating` default re-computes results but not traces. Every other op does nothing. Tables patch the
re-checked rows' cells only.

## Acceptance criteria

1. Design §11 checks 1–19 pass, including the worked examples 1–9 with the exact numbers given.
2. A project with no rating data shows no rating rows (no nagging).
3. No standard's table values in the repository: `ratingtable` has no built-in entries, and the
   only table rows are the synthetic fixtures in `tests/` (design §11 check 18). Every Table-basis
   result names its rating table and source.
4. MCP contract tests for `rating` and `ratingtable {csv}`; `ratingtable {import}` refused over MCP;
   `ratings` works with writes off.
5. Update-map counters on the medium project: a device rating change re-checks only its
   subjects, no table rebuild.
6. Boards 2.05 and 2.06 compared with screenshots of the same states.
7. `docs/quick-start/057-ratings.md` as design §12.

## Tester

**Yes.** These are safety-relevant calculations. The tester runs design §11 in full, recomputes
every worked example by hand from the synthetic fixtures and the formulas of §3 (not from the
code's output), tries the hostile values of §6.1, the CSV import of check 15 and checks 10 and 11, and runs the fast tests of `ecad-cadcraft`
and `ecad-trial`.

## Owner questions

Design 057 §14, questions 1–17: **answered** (owner, 2026-11-14). Q11: user entry only in 0.2.2,
no table values in the repository, a 0.4.0 standards review decides defaults. Q17: IEC is the
basis for method names and factors. All others as recommended.

## Needs owner check

Enter one of the owner's own licensed tables through Standards ▸ Rating tables (not committed) and
compare a few cables' results with a hand calculation.
