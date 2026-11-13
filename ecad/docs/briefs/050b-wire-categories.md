# Brief 050b: wire categories

Status: Approved (owner, 2026-11-13: design 050 §14 Q1–Q15 answered as recommended, including
roadmap Q25–29; overseer FP rulings of design §13: FP6 with one `ignored-old-setting` Checks row
for a potential's old `colour` / `cross_section`, no conversion).
Branch: `feature/style-model`, after 050a's commits on the same branch.
Designer: **yes** (`docs/design/050.md` §5, §6.9, §7.5) · Size: M
Roadmap: `docs/discussions/roadmap-review-2026-11.md` M1 item **1.4**, lane D, last. It depends on
1.13(a) (050a) and is not folded into it (roadmap §3.2; question 30).

## Goal

Every wire belongs to a **wire category**, such as power L/N, 24 V DC, 0 V, PE, signal or analogue.
The category gives the wire its colour, cross-section, line colour, line type and weight, and label
rule, and it is the default for new wires on its potential. Categories are a catalogue of 050a's
style model, so changing one changes every wire that uses it, live. The category shows on the wire
report and the BOM. A conductor's own `colour` and `cross_section` (022 FP3) stay, as overrides of
its category.

## What exists today (`feature/style-model` after 050a)

- **022:** the potentials table in `settings.wiring.potentials`, with name, prefix, numbered,
  colour and cross-section. Conductor properties `potential`, `colour` and `cross_section`. Labels
  `<number> <cross-section> <colour>`. The `wire` prompt's `[Potential]` keyword. `wiresettings`
  and `potential`.
- **040:** `wires::colour_routes` draws a wire in its colour code, with two-colour dashes, and
  `print.colour` mono (default) or colour.
- **024 / 047:** the wire report (`outputs::WireRow`, `wire_table`) and the BOM entries.
- **050a:** catalogues, `styles::field`, choice entries, the users index, the Standards catalogue
  strip (`CATALOGUES`), the shared style detail, and the inspector style rows.

## §0 Files touched

Design 050 §0.2 is the full table.

- **Owned:** `ecad-cadcraft/src/styles/wire.rs`, `apps/ecad-trial/src/lists/wire_categories.rs`,
  and `docs/quick-start/050b-wire-categories.md`.
- **Shared, by hunk:**
  - `ecad-model/src/wiring.rs` (`PotentialDef`, `WIRE_CATEGORY`);
  - `styles/mod.rs` and `defaults/styles.rs` (one line each);
  - `wires.rs`: the label text, and the route colour, linetype and weight in `colour_routes`. Lane
    D's 049 also edits `wires.rs`, so the overseer watches it;
  - `wire_cmds.rs` (`wire {category}`, `category`, `wiresettings`);
  - `outputs.rs` (`WireRow.category`, the wire report column, BOM wire rows);
  - `checks.rs` (two checks);
  - `project.rs` (one `pages_touched` arm);
  - the trial's `settings/standards.rs` (one `CATALOGUES` line) and `shell/right.rs` (wire rows);
  - the `lane_050.rs` test modules (extended).

## Scope, in order

1. **The `wire` catalogue:** fields and the ten built-in categories of design §5.1–§5.2, and the
   choice entry `wire.category` (S P O, built-in `none`).
2. **`wire_look`** (design §5.3), and the category/potential rule (§5.4). `PotentialDef` loses
   `colour` and `cross_section` (FP6).
3. **Drawing and labels:** labels and `colour_routes` read `wire_look`: line colour, type and
   weight, and the label rule. Mono print is unchanged (design §5.5).
4. **Commands:** `category`, `wire {category}` and the `[Category]` prompt keyword, and
   `wiresettings` refusing potential colours (design §6.9). MCP contract tests.
5. **Reports and the BOM:** the Category column, `nets` / `WireRow.category`, and BOM wire rows with
   the allowance (design §5.6).
6. **Standards and the inspector:** the Wire categories table and detail (the consequence line
   counts users and overrides), and the wire inspector's Category, Colour and Cross-section rows
   with Overridden and Reset (design §7.5).
7. **Checks:** `category-unknown`, `category-potential-twice`, and the potential kind of `ignored-old-setting` (overseer ruling on FP6, design §5.7).
8. **Bench** rows of design §11 for categories.

## Out of scope

Cable types and core colours per cable type (2.7). Label contents per category beyond the rule.
Wire lengths from a panel layout (3.1). Ferrule and end-sleeve counts (047's follow-up). The intent
line and wire wheel entries, if 041's intent line does not already show prompt keywords (a lane U
follow-up; the command line and MCP work regardless).

## Format and ops

FP5 (`wire.category`, a conductor property and registry choice) and FP6 (`PotentialDef` loses
`colour` and `cross_section`; old files open with them ignored), from design 050 §13. Category
records use 050a's `SetStyle`, with no new op. Pre-v1, so no migration. **Approved by the overseer
(2026-11-13), with the old-key check (design §13).**

## Update map

Design 050 §10, the `wire` rows:

- A category change marks stale only the pages of conductors resolving to it (explicit users, nets
  of its potential, and unclassified conductors when it is the project default). It patches those
  conductors' report rows and that category's BOM row from a kept count per category.
- `wire.category`, `potential`, `colour` and `cross_section` writes touch their own conductors and
  the two categories' counts.
- Every other op does nothing.

## Acceptance criteria

1. Design 050 §12 checks 11–18 pass.
2. On the sample project, wires on 022's default potentials look the same as on `dev`, apart from
   the power categories' line weight and ink, which the UI spec asks for (screenshot compare, with
   the differences listed in the quick start).
3. A conductor with its own `colour` keeps it through a category change. Its inspector row says
   "Overridden", and Reset returns it to the category.
4. A potential change on a net moves its unassigned conductors to the new potential's category, and
   leaves conductors with their own category alone.
5. MCP contract tests for `category` and `wire {category}`: the transaction, `Author::Agent`,
   refusal with writes off, and undo.
6. Update-map counters: a category change on the medium project redraws only the shown page,
   patches only the affected rows, and rebuilds no table.
7. `docs/quick-start/050b-wire-categories.md`: set the drawing-pack style (a 48 V AC category with a
   red line, power lines in their insulation colour), give one wire its own colour and reset it,
   and see the BOM wire row.

## Tester

**Yes.** FP6 changes `PotentialDef` in the file, and categories go through `SetStyle` in the store,
replay, merge and undo. The tester runs design §12 checks 11–18, check 18 in full, and the fast
tests of `ecad-model`, `ecad-cadcraft` and `ecad-trial`.

## Owner questions

Design 050 §14, questions 2–5 (line colour apart from insulation colour; the category names its
potential; a category choice sets a missing potential; BOM allowance).

## Needs owner check

Open the owner's own project, set its wire colours as categories in Standards ▸ Wire categories,
and compare page 2 with its print.
