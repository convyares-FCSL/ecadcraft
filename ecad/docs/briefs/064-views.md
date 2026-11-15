# Brief 064: One update pipeline for derived views (R9)

Status: Approved (owner, 2026-11-15: design 064 §14, all ten decisions as recommended). The
crate targets in step 2 and rule 3 will be renamed to lane 065's crate map once it is approved.
Branch: `feature/views`, from `origin/dev` after the check-kinds table (R7, lane 060) and the types
lane (`StyleCatalogue`, `keys.rs`) have merged.
Designer: **yes** (`docs/design/064-views.md`). Tester: **required** (commit path, `translate.rs`'s
caller, the ledger commit). Size: L, in 12 steps (below). Lanes: C (`project.rs` split,
`derived/`, every view module named in design §1.2), M (`ecad-model` `effect.rs`, `view.rs`,
`invariant.rs`), U (`checks_panel.rs`, `lists/cables.rs`).
Roadmap: 0.2.3 tidy-up, review finding 1, row R9. 058 (paged reports, 0.2.4) builds on it.

## Goal

Each derived view states its update map once, in a `note` function over one exhaustive
classification of the ops (`Effect`). All three commit paths (command, drawing commit,
undo/redo) call the same `Views::begin` / `finish`. A new `Op` doesn't compile until it is
classified. A new view takes one struct and one line in `Views`. Nothing a person sees changes,
except three stale-view gaps that close (design §5, D1–D3).

## Read first

- `docs/design/064-views.md`, all of it: it is short and every section is used.
- `crates/ecad-model/src/invariant.rs` (`Touched`), `op.rs`.
- `crates/ecad-cadcraft/src/project.rs`: `apply_tx`, `DocumentOwner::commit`, `history`,
  `generated_touched`, `update_outputs`, `pages_touched`.
- The functions in design §1.2's table, as you reach each one.
- Proposal Part A only (as `CLAUDE.md` says).

## Rules

- **Behaviour stays the same**, apart from D1–D3. Every view step deletes the old function and
  its calls in all three paths **in the same commit**, so the change log is never fed twice.
- `Effects::of` is the only `match` on `Op` that views rely on: no `_`, deny
  `clippy::wildcard_enum_match_arm` on `effect.rs` and `derived/`.
- `derived/` imports no `cadcraft_*` crate (it moves into ecad-domain whole at the crate split).
- Never-crash rules, `rustfmt` at 120, contract tests table-driven.
- No new op, format change, command or dependency. If one seems needed, stop and ask the overseer.
  rayon comes in only through step 12, and only if lane 061 has approved it.
- Per-lane files: tests in `tests/all/lanes/lane_064.rs` of each crate it tests, quick start in
  `docs/quick-start/064-views.md` (one paragraph: nothing to click, what was unified).

## Steps

Each step is one or more commits on `feature/views`. Each must pass `cargo fmt --check`,
`cargo xtask test-changed` and, before the push, clippy with all targets.

0. **Preconditions.** Check that R0's `Op::ALL` / `Op::name` coverage, `StyleCatalogue` and
   `keys.rs` are on `dev`; stop and report if not. Add a compile-time assert that `Document: Sync`.
   If it fails, record the blocking field in the quick-start note and continue without step 12.
1. **Split `project.rs`** into `handle_map.rs`, `page_tree.rs`, `persistence.rs`, `project/mod.rs`,
   `project/open.rs`, `project/changes.rs` and `project/owner.rs` (design §8). A pure move: no
   logic change, no visibility widened beyond `pub(crate)` / `pub(super)`. One commit, so
   `git diff -M` shows moves.
2. **The classifier and the trait.**
   - In ecad-model: `effect.rs` (`Effect`, `Placed`, `Key`, `Effects::of` with page, terminal
     and folder pairs folded, `EffectKind::ALL`, `EffectKindSet`, `Area`, `AreaSet`) and
     `view.rs` (`Phase`, `NoteCx`, `ViewScope`, `Outcome`, `PageSet`, `DerivedView`), exactly as
     design §3–§4. Rewrite `Touched::add` over `Effects`.
   - In ecad-cadcraft: `defaults::AREAS` comes from `Area::REGISTRY`, and the doubled
     `strippage.` doc line goes.
   - Add `derived/mod.rs` with an empty `Views`, `Pending`, `Changes`, `Lazy<V>`,
     `for_each_view`, and `Project::apply_changes`. Wire `begin` / `finish` / `apply_changes`
     into all three paths (design §5). With no views yet, `apply_changes` gets the old values
     passed through, so behaviour is unchanged.
   - Add an `xtask layers` rule: no `cadcraft_*` import under `derived/`.
   - Tests C1, C2, C3.
3. **Feeds.** `TreeFeed`, `DevicesFeed` and `PropsFeed` (design §4.1). This deletes
   `tree_touched`, `devices_touched`, `props::touched` and `with_numbered`. The numbering ops now
   reach the feeds through the After phase (D3).
4. **`FrameView`.** `frame::touches` over `Effect` (the folded `Page` replaces the pair search),
   `refresh_pending` moved in. This deletes `note_frame_ops` and its restore on refusal. It runs
   on the interactive path too (D1).
5. **`ConnectorsView`, then `RefsView`.** `xref::Watch` becomes the `Scope`. `cables_touched`,
   `names_named`, `xref::Scope::collect` and `connectors::Index::touched` / `pages_for` move to
   `note`, and the `xref_shown` reuse moves into `RefsView`. This deletes `finish_watch`. The
   orphan line goes to `Changes::echo`.
6. **`PagesView`.** `pages_touched` (with `plc::pages_touched`, `styles::pages_touched`,
   `unused_after` through `NoteCx::effects`) and `pages_named` → `Changes::named`. This deletes
   `numbered_pages` and the hand-added `page` in the interactive path. It runs on the interactive
   path (D2).
7. **`OutputsView`.** `outputs::Scope::collect`, `moves_page_numbers`, `plc::touched`,
   `strip_page` update and `rack_places_touched` move to `note` / `update`. `OutputsOutcome`
   carries the per-kind changed flags of design §4.2. This deletes `update_outputs`.
8. **`ChecksView`.** `checks::Scope::collect` with page types, style rows, title-block rows and
   `rating::note` moves to `note`, and the cover rows to `update`. This deletes `update_checks` and
   the `set_cover` call in `generated_touched`.
9. **`GeneratedView`.** One exhaustive `match GenKind` (design §4.2) and the cover `Uses`. This
   deletes `generated_touched`, `generated::touches` and `update_outputs`' `rewritten` list.
   Test C8.
10. **The guarantee.** Test C4 (`update_map_matrix`) with its allowlist, plus
    `cargo xtask update-maps` writing `docs/design/064-update-maps.md`. Add the `xtask layers`
    ratchet against `starts_with("` and `split_once('.')` on keys under `derived/`, and one
    `update` / rebuild counter per view. Tests C5, C6, C7.
11. **Trial consumers** (design §6). `Checks` keeps its rows in book order and returns a row
    delta, plus `rows_of(Object)`. Add `ChecksPanel::sync` patching and the `sorts` counter.
    The Cables tab reads open cores from `OutputsView`'s cable rows. Tests C9, C10.
12. **Parallel seam** (design §5.1). Run stages through `for_each_view`. Add the `parallel`
    feature (rayon, off by default, never on wasm) **only if 061 approved rayon**; otherwise
    leave the loop and say so in the quick-start note. Test C11 under the feature.

At the end: run the large bench once for the `views.*` and `checks_panel.sync` rows (design §10).

## Acceptance criteria

1. `Op` is matched for derived state only in `Effects::of`. Design §1.2's 22 functions are gone,
   or have become `note` bodies over `Effect`. `grep -n "Op::" crates/ecad-cadcraft/src/derived`
   finds nothing.
2. `apply_tx`, `DocumentOwner::commit` and `history` each call `Views::begin`, `Views::finish`
   and `Project::apply_changes`, and none of them calls a view directly.
3. Adding a variant to `Op` without classifying it fails to compile (shown in the PR description
   by a one-line experiment, not committed).
4. Adding a variant to `GenKind` without a source in `GeneratedView::changed` fails to compile.
5. Tests C1–C10 pass, plus C11 if step 12 added the feature. C12: the existing fast tests of
   ecad-model, ecad-cadcraft, ecad-mcp and ecad-trial pass unchanged.
6. `064-update-maps.md` matches the code (C4). `ALL` appears only on the allowlist rows.
7. Bench: the `views.*` rows are no worse than `dev` +10% on medium. `views.overhead` is under
   0.2 ms on medium, and `checks_panel.sync` under 1 ms on large.
8. MCP: no command is added or changed, so there is no new MCP contract test. The existing
   `ecad-mcp` contracts pass.
9. `derived/` imports no `cadcraft_*` crate (`cargo xtask layers`).

## Out of scope

Ops, the format, commands, anything a person sees. Turning trial lists into `DerivedView`s.
Parallelism inside Checks or reports. Tag high-water and designation uses. Narrowing
`Document`'s fields (R10). 058's layouts, which build on this afterwards (design §11).

## Owner decisions

Design 064 §14, decisions 1–10: approved as recommended (owner, 2026-11-15). That covers D1–D3
fixed in this lane, T1 and T2 in step 11, and the bench targets: no worse than `dev` +10% on the
edit rows, and pipeline overhead under 0.2 ms on medium.
