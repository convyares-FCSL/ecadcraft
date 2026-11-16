# Brief 072: Types for keys, units, words and device kinds

Status: **Draft** (designer, 2026-11-16; revised after the owner's answers). Needs the owner's
calls in design 072 §14 and the overseer's approval of F3′ and F8–F12 (design §13) before it is
approved. F1, F2, F4–F7 approved by the overseer, 2026-11-16.
Branch: `feature/types`, from `refactor/0.2.3` **after lanes 070 (errors and typed command
inputs) and 073 (renames) have landed there**. Phase: 0.2.3 tidy-up; no PRs: the overseer merges
the lane when it is green.
Designer: **yes** (`docs/design/072-types.md`, the spec) · Tester: **yes** (a new op and format
changes) · Size: L, in eight steps.

## Goal

Property keys, units, stored words and device kinds stop being free strings and bare numbers, so
the compiler finds every place a new key, unit, catalogue or device kind must be handled. Code
review 2026-11, findings 4, 6, 7 and 10 (lanes R4, R5, R8), with the owner's answers of
2026-11-16: unknown words are a read error, and a device's kind is data declared by its symbol.

## Why

- About 105 key constants in 27 files, 42 literal `"tag"` keys, 151 hand-split keys, three
  different boolean word sets and about 30 digit checks (design §0).
- 595 string-literal match arms; 11 `match key` blocks over 113 starter symbols; style catalogues,
  roles and connector sides as strings.
- Ten device predicates, each working out from the symbol what a device is, and about 30
  kind-only fields in the generic property bag (a strip could hold a PLC slot).
- Angles in degrees and radians mixed (one command takes radians); ratings parsers return bare
  `f64` for A, V, mm², °C and %.

## Scope

Design 072 is the spec; read it whole. In short:

1. `ecad_model::words`: the one yes/no rule, whole-number readers, `Word`; strict enums (§2.2,
   §4.1).
2. `ecad_model::keys`: `PropKey`, `Area`, `KEYS`, `FAMILIES`, `RESERVED`, `PropsExt`, built-key
   constructors; every generic key constant and literal moved there (§2).
3. Defaults: `Entry.key: PropKey`, `Kind::Flag`, typed `Choice<E>` (§2.5, §4.8).
4. `ecad_model::units` and page places, entered at the parsers, `Placement`, `Place` and 070's
   params (§3, §7).
5. Enums: `StyleCatalogue`, `Side`, `TextKind`/`Shown`, `StarterKey` (stays in electrical),
   `SymbolKey`, `SymbolName`, `PartRole`/`Role` (§4).
6. Unknown words: a damaged record for typed fields; the open-time word check and
   `ReadOnly::UnknownWords` for words in values (§4.1).
7. `DeviceKind` on `Component` and `Definition`, the new op `SetDeviceKind`, kind-only fields moved
   into the variants, the predicates deleted (§5).
8. Guards G1–G10, all at 0 with no allowlist at the end (§6).

Format and op changes are design §13: F1, F2, F4–F7 approved; F3′ and F8–F12 are part of this lane
once the overseer approves them. Fixtures are regenerated, never shimmed (§5.8).

## Not in this lane

`Pt` and drawing-unit coordinates; `SetProperty.key`; kinds for cables, wires, pages or folders;
an import step for other tools' files; the check-kinds table (R7); the views (R9); 070's lenient
JSON readers; anything outside `ecad/`.

## Steps

Each step is one or more commits `072 <step>: …`, each green: builds, clippy clean, tests of the
crates it touched pass, every guard at or under its count. Push after each green commit.

1. **Vocabulary (additive).** strum as a workspace dependency of ecad-model and ecad-electrical.
   Add `words.rs`, `units.rs`, `keys.rs` with their unit tests (A1–A4). Add guards G2, G3, G8, G10
   to `cargo xtask layers` as ratchets at today's exact counts (written in the commit message).
   No caller changes yet.
2. **Generic property keys, one area per commit:** identity and tag; book and page; connectivity
   and wiring; BOM; rating circuit keys; revisions and designations; front; text and symbol
   families. Kind-only keys (design §5.3) wait for step 7. Each commit moves the area's constants
   into `keys.rs`, switches its readers to `PropsExt` / `words`, and lowers the ratchets. The first
   commit adds G1 (the lane_072 test). **Stop and report** after the first two areas so the
   overseer can check the pattern. Ends with G1 (generic keys), G2, G3 at 0.
3. **Defaults.** `Entry.key: PropKey`; `Kind::Flag` for the 25 switches; `Choice<E>` with the §4.8
   enums; the TOML files regenerated with `yes`/`no`; the Standards screen's switches as
   `control::Toggle`. G8 at 0.
4. **Units.** `rating/parse.rs` typed; `Placement`, `TextSlot`, `Primitive::Text` → `Radians`
   (round-trip test first: the stored bytes must not change); 070's params → `Degrees`, `Mm`,
   `Metres`; symbol content `slots[].rotation` in degrees (F6, with its MCP contract test);
   `Value::Length` → `Mm`; `Grid`/`Place` → `PageColumn`/`PageRow`/`PageNo`. G7 at 0.
5. **Small enums and unknown words.** `StyleCatalogue` in `Op::SetStyle` and `StyleCatalogues`
   (F1, F2), `Side` into ecad-model, `TextKind` and `Shown`; `words_check` and
   `ReadOnly::UnknownWords` on the open path, unknown typed words reported as damaged records (F11,
   A5). *The overseer may merge the lane here if R9 is ready to start.*
6. **Starters and roles.** `StarterKey` with `STARTERS` keyed by it (in electrical); the 11
   `match key` blocks exhaustive (G5); `SymbolKey`, `SymbolName`, `cable_prefix` (F5); `PartRole`
   and `Role` (F3′). G9 at 0; G4 at 0 except `DeviceKind` words.
7. **Device kinds.**
   - (a) `kind.rs`, `Component.kind` and `Definition.kind`, every kind `General` at first;
     `SetDeviceKind` with its contract tests (A11); starters declare their kinds and placement sets
     the kind (A10); the symbol builder's kind choice (F8, F9, F12). The oracle test A9 records
     today's predicate answers on the test projects and the medium bench project.
   - (b) One kind per commit, in this order: Junction and Continuation, LocationBox, Item, Relay,
     Protective, Connector, TerminalStrip, PlcCard. Each moves its fields out of the bag into the
     variant (design §5.3), moves its readers, commands (through `SetDeviceKind`) and update-map
     rows (§11), adds its keys to `RESERVED`, deletes its predicate, and regenerates the fixtures
     it touches (F10, §5.8). A9 stays green until the predicate is gone.
   - (c) G6 and G4 (all words) at 0.
8. **Close.** Every guard is a ban with no allowlist entry; `docs/quick-start/072-types.md`;
   remaining fixtures regenerated; the bench rows of design §10 (medium while building, large once
   now).

## Acceptance

Design 072 §10, A1–A14, in `tests/all/lanes/lane_072.rs` of each crate tested (plus one line in
`tests/all/lanes/mod.rs`). Every command whose input type or recorded op changed has the MCP
contract test of brief 011 (accepted call, its transaction and `Author::Agent(..)`, refused with
writes off). The inspector's Raw and Readable views show the changed words and the kinds (A12).

## Stop and ask the overseer

- A device whose new kind differs from today's predicates (A9), beyond the fuse case the owner
  decides in §14.
- A kind-only key that turns out to sit on more than one kind, or a holder the design didn't list
  (§5.3).
- A drawn form (`Form::Drawn`) that needs its starter, which the role no longer carries (§4.4).
- Any stored word or field change not in design §13, or any op beyond `SetDeviceKind`.
- A guard that cannot reach 0 without a change outside this scope.

## Checks per commit

`cargo fmt --all -- --check`, `cargo xtask test-changed` (or the touched crates by name when the
workspace `Cargo.toml` changed, as in step 1), `cargo xtask layers`. Before each push: `cargo
clippy --workspace --all-targets --all-features -- -D warnings`. Long output to a log file under
`C:\dev\ecad-features\_tmp\072-logs`. Never the whole workspace's tests and never the slow set.
