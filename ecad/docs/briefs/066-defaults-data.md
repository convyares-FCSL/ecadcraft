# Brief 066: Defaults as data

Status: **Approved** (owner, 2026-11-15: design 066 §9 answered; overseer, 2026-11-15: format
changes F2 and F3 approved).
Branch: the 066 lane branch, from `refactor/0.2.3` **after lane 067 (ecad-electrical) lands
there**. Phase: 0.2.3 tidy-up; no PRs: the overseer merges the lane when it is green (owner
process for 0.2.3).
Designer: **yes** (`docs/design/066-defaults-data.md`) · Size: M, in three parts (066a M, 066b S,
066c S)
Runs alongside: 060 (guards), 062 (tests), 063 (UI kit). Touches the registry (moves from
`crates/ecad-cadcraft/src/defaults/` to `ecad-electrical`), `styles/*`, `ecad-model` (`wiring.rs`,
`doc.rs`), `ecad-file` (`v1.rs` settings).

## Goal

Built-in defaults stop being Rust literals and become TOML data files built into ECAD (owner,
2026-11-15). A value a project chose stays chosen, even when it equals a built-in, and a project
reads the same on two machines whatever company files they hold. The first value changed as data
is the wire label cap height, 1.6 → 2.5 mm.

## Why

- Owner ruling: defaults come from configuration data, not code.
- Lane 060 found that a project leaves out settings equal to the built-in, and the registry treats
  "equal to the built-in" as "not set". So a project that chose 1.6 cannot be told from one that
  never chose, and would move to 2.5 with everyone else.

## Scope

Design 066 is the spec. In short:

- **066a. The built-ins as TOML** (design §2, §3, §7). Every registry entry's built-in value and
  every built-in style field value moves into `ecad-electrical/defaults/<area>.toml`, one file per
  area, embedded with `include_str!`. Kind, range, levels, label stay in the Rust `Entry`; its
  `builtin` field goes. One loader and validator with a `thiserror` `DefaultsError` carrying file,
  line, column and byte span; parsed once into a table indexed by entry. `DEFAULT_LABEL_HEIGHT`,
  `DEFAULT_LABEL_RULE`, `DEFAULT_LABEL_MIN_LENGTH` removed; their readers ask the registry. The
  hard-coded text heights of design §2.4 read their styles, keeping what pages draw today.
  **Nothing visible changes.**
- **066b. The saved-file rule** (design §5, F2). `wiring.label_height`, `label_rule`,
  `label_min_length` become optional (absent = built-in, present = chosen); the "equal to the
  built-in" readers in `defaults/text.rs` and `wires.rs` go. Then `text.wire_label.height` becomes
  **2.5** in `text.toml`.
- **066c. The other settings defaults** (design §2.2, F3): tag padding, wire number digits, page
  number step, tag dash and case, sheet, reference and colour style, report row and header heights
  become registry entries with values in the TOML and optional fields; rows per sheet is computed.

## Out of scope

- Numbered built-in sets, `ProjectSettings.defaults`, `sets.lock`, `usedefaults`, **Keep how it
  looks**: deferred to the v1 format freeze (`docs/deferred-work.md`; design §10).
- Loading a company standard from a file: waits for libraries (stage 3).
- Title block columns, cover sizes, terminal strip drawing look, line weights (design §2.3): later
  briefs, listed in `docs/deferred-work.md`.
- `miette` (lane 061).

## Step order

066a → 066b → 066c. Each part is green on its own: `cargo fmt`, `cargo xtask test-changed`, clippy
before push. Bench the medium project's text layout after 066a: resolving must stay a lookup by
entry index, no regression.

## Acceptance

Tests in `tests/all/lanes/lane_066.rs` of each crate touched (design §8 lists them in full):

1. **Data complete and valid:** every entry has a value; every key in the files is an entry;
   every value, and every built-in style field value, is valid for its kind.
2. **No visible change after 066a:** the screenshot and projection tests already on the branch pass
   unchanged.
3. **Round trip** (v1 and v2): a chosen `label_height = 1.6` and an unset height both survive save
   and open; every entry resolves the same.
4. **Chosen equals built-in:** with the built-in at 2.5, a project that chose 1.6 reads 1.6; a
   project that chose 2.5 keeps the field present.
5. **Not chosen follows:** a project with no chosen height draws 2.5 mm wire labels after 066b.
6. **Two machines:** the same project opened with two different company standard files on the
   machine (and none) resolves every entry the same and projects the same page.
7. **Hostile files:** the list in design §8, each refused with its `DefaultsError`, no panic,
   resolution falls through to the next layer.
8. `defaults` and `setdefault` keep their behaviour and tests (no new command, so no new MCP
   contract).

Tester: **yes** (`.ecad` format change, F2 and F3). Quick start:
`docs/quick-start/066-defaults-data.md` (open an existing project and see 2.5 mm wire labels; set a
project's wire label height to 1.6, save, reopen and see 1.6 kept).

## Owner answers

Design 066 §9 (owner, 2026-11-15): 12 no (one set; every project reads the current built-ins);
13 and 15 deferred with numbered sets to the v1 format freeze; 14 the company file waits for
libraries; 16 later moves deferred as listed. ECAD is desktop-only. Overseer: F2 and F3 approved;
the registry goes into ecad-electrical (lane 067).
