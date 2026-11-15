# Brief 066: Defaults as data

Status: Draft (designer, 2026-11-15). Needs the owner's answers to design 066 §9 (12–16) and the
overseer's call on format changes F1–F3 (design §5.4) before it is approved.
Branch: a feature branch named by the overseer, from `origin/dev`. Phase: 0.2.3 tidy-up.
Designer: **yes** (`docs/design/066-defaults-data.md`) · Size: L, in four parts (066a M, 066b M,
066c S, 066d S, the last only if the owner approves it)
Runs alongside: 060 (guards), 062 (tests), 063 (UI kit), 065 (crate map). Touches the registry
(`crates/ecad-cadcraft/src/defaults/`, `styles/`), `ecad-model` (`wiring.rs`, `doc.rs`), `ecad-file`
(`v1.rs` settings), Settings ▸ Standards.

## Goal

Built-in defaults stop being Rust literals and become data files shipped inside ECAD (owner,
2026-11-15). A saved project reads the same on any machine and any later build: changing a
built-in, or having a different company file, never changes an existing project silently. The
first value to change is the wire label cap height, 1.6 → 2.5 mm, for new projects.

## Why

- Owner ruling: defaults come from configuration data, not code.
- Lane 060 found that a project leaves out values equal to the built-in, so changing a built-in
  moves every existing project. The registry also treats "equal to the built-in" as "not set", so a
  project that chose 1.6 would move to 2.5 too.

## Scope

Read design 066 in full; it is the spec. In short:

- **066a. Set 1 as TOML** (§3, §7). Every registry entry's built-in value and every built-in style
  field value moves into `set-1/<area>.toml`, embedded with `include_str!`. Kind, range, levels,
  label stay in the Rust `Entry`; its `builtin` field goes. One loader and validator with a
  `thiserror` `DefaultsError` carrying file, line, column and byte span. `sets.lock` and its test.
  `DEFAULT_LABEL_HEIGHT`, `DEFAULT_LABEL_RULE`, `DEFAULT_LABEL_MIN_LENGTH` removed; their readers
  ask the registry. The hard-coded text heights of design §2.4 read their styles, keeping what pages
  draw today. **Nothing visible changes.**
- **066b. Named sets and the saved-file rule** (§5). `ProjectSettings.defaults` (always written;
  absent and `Default` = set 1); new projects record the newest set in the "New project"
  transaction. `wiring.label_height`, `label_rule`, `label_min_length` become optional (absent =
  the set's value); the "equal to the built-in" readers in `defaults/text.rs` and `wires.rs` go.
  `set-2.toml` changes `text.wire_label.height` to 2.5. A file naming an unknown set is refused
  (§5.3). Command `usedefaults` and the Standards line and dialog (§5.2), with **Keep how it looks**.
- **066c. The other settings defaults** (§2.2, F3): tag padding, wire number digits, page number
  step, tag dash and case, sheet, reference and colour style, report row and header heights become
  registry entries with values in set 1 and optional fields; rows per sheet is computed.
- **066d. Company standard file** (§4.1), **only if the owner approves it** (design §9, call 14).

The registry moves to the Core crate lane 065 names (§6) if 065 has landed; otherwise it changes in
place and moves with 065.

## Out of scope

- Title block columns, cover sizes, terminal strip drawing look, line weights (design §2.3): listed
  in `docs/deferred-work.md`, each its own later brief.
- `miette` (lane 061). Libraries and `.ecadlib` (stage 3).
- Converting existing projects: they keep set 1 (no compatibility work pre-v1 is needed; none is
  done).

## Step order

066a → 066b → 066c (→ 066d). Each part is green on its own: `cargo fmt`, `cargo xtask
test-changed`, clippy before push, and `cargo xtask wasm` once (066a adds `toml` to a Core crate).
Bench the medium project's text layout after 066a: resolving must stay a lookup by entry index in
the project's set, no regression.

## Acceptance

Tests in `tests/all/lanes/lane_066.rs` of each crate touched (design §8 lists them in full):

1. **Sets complete and valid:** every entry has a set-1 value, every set key is an entry, every
   value is valid for its kind.
2. **Lock:** changing a released value fails with the "add a new set" message; additions pass.
3. **No visible change after 066a:** the screenshot and projection tests already on `dev` pass
   unchanged.
4. **Round trip** (v1 and v2): `defaults`, a chosen `label_height = 1.6`, and an unset height all
   survive save and open; every entry resolves the same.
5. **Old file:** a pre-066 fixture reads with set 1; wire labels 1.6.
6. **New project:** wire labels 2.5; its history replays to set 2.
7. **Chosen equals built-in:** a project that chose 1.6 still reads 1.6 after moving to set 2.
8. **Two machines:** the same project opened with two different company standards folders (and
   none) resolves every entry the same and projects the same page.
9. **Hostile files:** the list in design §8, each refused with its `DefaultsError`, no panic,
   resolution falls through to the next layer.
10. **Newer set:** a file naming set 99 is refused with the design §5.3 message.
11. **`usedefaults`:** shows the differences; `keep=on` moves nothing on the page, `keep=off`
    gives 2.5 labels; one transaction; undo reverts it. **MCP contract test:** through `ecad-mcp`'s
    `execute`, the transaction is recorded with `Author::Agent(<client name>)`, and the command is
    refused with writes off.
12. **UI:** Settings ▸ Standards shows "Built-in defaults: set *n* (newest: *m*)" and **Use newest
    defaults…** only when the project is not on the newest set; the dialog uses `ecad_ui`
    components only (`table`, `control::Toggle`, `prim::Button`, `control::read_text`), element
    names as in design §5.2.

Tester: **yes** (`.ecad` format change, F1–F3). Quick start: `docs/quick-start/066-defaults-data.md`
(make a new project and see 2.5 labels; open an older project and see 1.6; move it with **Use newest
defaults…**, with and without **Keep how it looks**).

## Owner answers

Pending: design 066 §9, calls 12–16. Overseer: F1–F3 (design §5.4).
