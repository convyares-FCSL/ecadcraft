# Brief 037: UI tokens, primitives and fonts (U1)

Status: Approved (owner, 2026-11-13: U0 questions Q1–Q4 as recommended, except Q1: fonts live in ECAD's repo, not craft-fonts).
Branch: `feature/ui-tokens`, from `origin/dev` once this brief is approved.
Designer: done (`docs/design/ui-architecture.md` §2–§4; the look is spec §4) · Size: M
Roadmap: M1 **1.5** (UI shell), first part; the U programme's foundation lane, which merges into
`dev` before U2 and U3 (owner, 2026-11-13).
Inputs: `docs/discussions/ui-2026-11/ui-design-spec.md` §4 and §6 step 1, `owner-answers.md`.

## Goal

One place for every colour, size, radius, line weight and font of ECAD's interface, and the nine
widgets built from it, so that from here on nothing in ECAD is styled locally. No layout changes:
the app looks almost the same after this brief, except where a primitive replaces a hand-made
widget that is explicitly listed below.

## §0 Files touched

| File | Change |
| --- | --- |
| `ecad/crates/ecad-tokens/` (new crate: `Cargo.toml`, `src/lib.rs`, `src/{shell,sheet,marker,wire,ui_type,sheet_type,space,size,radius,line,wheel,shadow,icon}.rs`, `tests/all/`) | The token table (design §3.1). No dependencies. `[lints] workspace = true`; the crate-root `deny` attribute |
| `ecad/crates/ecad-ui/` (new crate: `Cargo.toml`, `src/lib.rs`, `src/style.rs`, `src/fonts.rs`, `src/prim/*.rs`, `src/icons/*.rs`, `tests/all/`) | Style, fonts, the nine primitives, icon painters (design §3.2–§3.4, §4). Depends on `ecad-tokens`, `egui`, `epaint`, `egui_extras` only |
| `ecad/Cargo.toml` | Two workspace members; `egui_extras` already patched to the fork |
| `ecad/xtask/src/main.rs` | New layer class **UiKit** and the two crates in `TABLE`; both in `WASM_CRATES`; the "no local styling" source check with a shrinking allowlist (design §3.2) |
| `crates/ui-egui/src/theme.rs` (**upstream seam US2**, `// ECAD-SEAM(owner): font definitions builder.`) | `pub fn font_definitions(language) -> FontDefinitions`; `install_fonts` becomes `ctx.set_fonts(font_definitions(language))`, behaviour unchanged |
| `crates/ui-egui/tests/` (one module) | US2: `install_fonts` sets exactly `font_definitions`'s result |
| `ecad/UPSTREAM-PATCHES.md` | Seam US2 |
| `ecad/assets/fonts/` (new: IBM Plex Sans 400/500/600, Barlow Semi Condensed 400/600, `OFL.txt` for each family) | The five faces, taken unmodified from the official sources (`github.com/IBM/plex` releases, `github.com/google/fonts` `ofl/barlowsemicondensed`); record the source URL, version and SHA-256 of each file in `ecad/assets/fonts/README.md` |
| `ecad/apps/ecad-trial/src/main.rs`, `lib.rs` | Call `ecad_ui::style::apply` and `ecad_ui::fonts::install` at start (after CADCraft's) |
| `ecad/docs/design/tokens.md` | Rewritten to spec §4's names, keeping every existing `ecad.*` row (design §12 O6) |
| `ATTRIBUTION.md` | IBM Plex Sans and Barlow Semi Condensed, SIL OFL 1.1 (icons are drawn in code) |

Not touched: `ecad-cadcraft`, the canvas, the shell layout, any command. `Embedding` (US1) is
U2's.

## Scope

1. **`ecad-tokens`**: spec §4 as constants (design §3.1): shell colours, paper and dark sheet
   sets with `prints(meaning)`, marker shapes and sizes, wire category colours, UI type ramp,
   sheet text styles as cap heights with `font_size_mm()`, spacing, sizes, radii, line weights,
   wheel geometry, shadow, and the `Icon` name enum.
2. **`ecad-ui::style`**: egui visuals, spacing and text styles for both egui themes from
   `GRAPHITE`; one appearance (no OS light or dark following).
3. **`ecad-ui::fonts`**: register IBM Plex Sans as the first `Proportional` face and families
   `ui` and `sheet`, on top of CADCraft's `font_definitions(language)`. Bytes are embedded from
   `ecad/assets/fonts/` with `include_bytes!` (native and wasm), so the faces are always present;
   no craft-fonts dependency and no fallback path. Offering them to `storytold/craft-fonts` is a
   later, owner-approved upstream step.
4. **Primitives**: button, icon button, pill, chip, badge, field, segmented control, key cap,
   row; plus `read_value` and `provisional` (design §3.3). Each has an accessible label.
5. **Icons**: painter functions for the shell's icons (design §3.4).
6. **No local styling**: the xtask source check and its allowlist, seeded with today's offenders
   in `ecad-trial`.
7. **Showcase**: a hidden `ui.primitives` page in the Text panel's place (today: the Developer
   fold) that draws every primitive in every state, for the tester's screenshots.

## Out of scope

Shell layout and regions (U2), the table (U3), canvas colours and the sheet font switch (U4a),
the wheel widget (U4b), restyling existing panels beyond what the allowlist forces (each U brief
removes its own entries).

## Update map

No view: tokens are constants and styles are applied once at start. Nothing reacts to commits.

## Acceptance criteria

1. `cargo test -p ecad-tokens` passes and includes: every token row of `tokens.md`'s new tables
   equals its constant (name and value); every text colour on its stated background meets 4.5:1
   (`text_3` on `raised` 4.5:1 or better, as spec §4 says 4.6:1); `prints()` is true exactly for
   ink, ink 2, cable, provisional, terminal link and the missing-data warning; cap height 2.5 mm
   gives 3.57 mm font size (to 0.01).
2. `cargo test -p ecad-ui` passes without wgpu and in under 5 s after build: each primitive in
   each state is found by its label; a disabled button reports its reason; `field` shows the
   focus ring colour `accent_text` 2 px; `read_value` has no frame and is clickable; every `Icon`
   paints inside its rect with 1.5 px strokes.
3. `cargo xtask layers` passes, and fails (tested) when `ecad-ui` gains a `cadcraft-*` dependency
   or when a non-allowlisted ECAD UI file adds `Color32::from_rgb`.
4. `cargo xtask wasm` builds `ecad-tokens` and `ecad-ui`.
5. US2: CADCraft's own tests pass unchanged; the new US2 test passes; `cargo xtask patches`
   passes.
6. The trial's UI text is drawn in IBM Plex Sans (screenshot of the showcase, read by eye), and a
   test checks that `ui` and `sheet` families are registered with the embedded faces. The
   `README.md` hashes match the committed files (test).
7. The trial's existing tests pass unchanged (`cargo xtask test-changed`), apart from tests that
   asserted a style value a primitive replaced, each named in the PR.
8. The PR records the measured size of the five font faces.

## Tester

Not needed (no format, op or store change). The reviewer checks the seam and the layering.

## Owner check

Look at the showcase screenshot beside the spec's Tokens board.
