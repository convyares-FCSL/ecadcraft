# Brief 072: Types for keys, units, kinds and device classes

Status: **Draft** (designer, 2026-11-16). Needs the owner's answers to design 072 §14 before it
is approved. Format list (design §13, F1–F7) approved by the overseer, 2026-11-16.
Branch: `feature/types`, from `refactor/0.2.3` **after lanes 070 (errors and typed command
inputs) and 073 (renames) have landed there**. Phase: 0.2.3 tidy-up; no PRs: the overseer merges
the lane when it is green.
Designer: **yes** (`docs/design/072-types.md`, the spec) · Tester: **yes** (op and format
changes) · Size: L, in eight steps.

## Goal

Property keys, units, stored words and device kinds stop being free strings and bare numbers, so
the compiler finds every place a new key, unit, catalogue or device kind must be handled. Code
review 2026-11, findings 4, 6, 7 and 10 (lanes R4, R5, R8).

## Why

- About 105 key constants in 27 files, 42 literal `"tag"` keys, 151 hand-split keys, three
  different boolean word sets and about 30 digit checks (design §0).
- 595 string-literal match arms; 11 `match key` blocks over 113 starter symbols; style catalogues,
  roles and connector sides as strings.
- Ten device predicates, each walking part → definition → starter; adding a device kind touches
  10–12 files.
- Angles in degrees and radians mixed (one command takes radians); ratings parsers return bare
  `f64` for A, V, mm², °C and %.

## Scope

Design 072 is the spec; read it whole. In short:

1. `ecad_model::words`: the one yes/no rule, whole-number readers, `Word`, `Stored<E>` (§2.2, §4.1).
2. `ecad_model::keys`: `PropKey`, `Area`, `KEYS`, `FAMILIES`, `PropsExt`, built-key constructors;
   every key constant and literal moved there (§2).
3. Defaults: `Entry.key: PropKey`, `Kind::Flag`, typed `Choice<E>` (§2.5, §4.8).
4. `ecad_model::units` and page places, entered at the parsers, `Placement`, `Place` and 070's
   params (§3, §7).
5. Enums: `StyleCatalogue`, `Side`, `TextKind`/`Shown`, `ComponentKind`, `StarterKey`,
   `SymbolKey`, `PrefixKey`, `Role` (§4).
6. `ComponentClass` / `DeviceClass` kept in the model's index; the predicates deleted (§5).
7. Guards G1–G9, all at 0 with no allowlist at the end (§6).

Format and op changes F1–F7 (design §13, approved by the overseer 2026-11-16) are part of this
lane.
Fixtures are regenerated, never shimmed (owner, 2026-11-16: no compatibility path).

## Not in this lane

`Pt` and drawing-unit coordinates; `Properties` storage and `SetProperty.key`; the check-kinds
table (R7); the views (R9); 070's lenient JSON readers; anything outside `ecad/`.

## Steps

Each step is one or more commits `072 <step>: …`, each green: builds, clippy clean, tests of the
crates it touched pass, every guard at or under its count. Push after each green commit.

1. **Vocabulary (additive).** strum as a workspace dependency of ecad-model and ecad-electrical.
   Add `words.rs`, `units.rs`, `keys.rs` with their unit tests (A1–A4). Add guards G2, G3, G8 to
   `cargo xtask layers` as ratchets at today's exact counts (write them in the commit message).
   No caller changes yet.
2. **Property keys, one area per commit:** identity and tag; book and page; connectivity and
   wiring; BOM; PLC; connectors; rating; revisions and designations; strips and accessories;
   front; text and symbol families. Each commit moves the area's constants into `keys.rs`, switches
   its readers to `PropsExt` / `words`, and lowers the ratchets. The first commit adds G1 (the
   lane_072 test). **Stop and report** after the first two areas so the overseer can check the
   pattern, as 070 did. Ends with G1, G2, G3 at 0.
3. **Defaults.** `Entry.key: PropKey`; `Kind::Flag` for the 25 switches; `Choice<E>` with the §4.8
   enums; the TOML files regenerated with `yes`/`no`; the Standards screen's switches as
   `control::Toggle`. G8 at 0.
4. **Units.** `rating/parse.rs` typed; `Placement`, `TextSlot`, `Primitive::Text` → `Radians`
   (round-trip test first: the stored bytes must not change); 070's params → `Degrees`, `Mm`,
   `Metres`; symbol content `slots[].rotation` in degrees (F6, with its MCP contract test);
   `Value::Length` → `Mm`; `Grid`/`Place` → `PageColumn`/`PageRow`/`PageNo`. G7 at 0.
5. **Small enums.** `StyleCatalogue` in `Op::SetStyle` and `StyleCatalogues` (F1, F2), `Side`,
   `TextKind` and `Shown`, `ComponentKind`. G9's catalogue part at 0. *The overseer may merge the
   lane here if R9 is ready to start.*
6. **Starters.** `StarterKey` and `CardType` into `ecad-model/src/starter.rs`; `STARTERS` keyed by
   it; the 11 `match key` blocks exhaustive (G5); `SymbolKey`, `PrefixKey`, `cable_prefix` (F5);
   `Role` (F3). G4 and G9 at 0.
7. **Device class.** `class.rs`, the index's `classes` with the §5.3 update map,
   `Document::class_of`. First the oracle test A9 against the old predicates; report any
   disagreement (especially the two fuse predicates) **before** deleting them. Then the predicates
   and `wires::kind_of` go and their callers use the class. A10. G6 at 0.
8. **Close.** Every guard is a ban with no allowlist entry; `docs/quick-start/072-types.md`;
   regenerate any remaining fixtures; the bench rows of design §10 (medium while building, large
   once now).

## Acceptance

Design 072 §10, A1–A13, in `tests/all/lanes/lane_072.rs` of each crate tested (plus one line in
`tests/all/lanes/mod.rs`). Every command whose input type changed has the MCP contract test of
brief 011 (accepted call, its transaction and `Author::Agent(..)`, refused with writes off). The
inspector's Raw and Readable views show the changed words (A11).

## Stop and ask the overseer

- Any class answer that differs from today's predicates (A9).
- A starter whose class is unclear (e.g. the `rj45`/`usba`/`usbb` bulkheads, design §5.1).
- Any stored word change not in design §13, or any new op.
- A guard that cannot reach 0 without a change outside this scope.

## Checks per commit

`cargo fmt --all -- --check`, `cargo xtask test-changed` (or the touched crates by name when the
workspace `Cargo.toml` changed, as in step 1), `cargo xtask layers`. Before each push: `cargo
clippy --workspace --all-targets --all-features -- -D warnings`. Long output to a log file under
`C:\dev\ecad-features\_tmp\072-logs`. Never the whole workspace's tests and never the slow set.
