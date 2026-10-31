# Brief 007: Upstream sync to the latest CADCraft

Status: Draft (overseer, 2026-10-31; requested by the owner as part of the baseline)
Branch: `upstream-sync/2026-10-31`
Designer: no · Size: L
Depends on: 001 (so both Linux and Windows CI judge the result)

## Goal

ECAD runs on the latest upstream CADCraft. The generic extension interface (X1-X8) is carried over
intact, and nothing ECAD-specific enters upstream crates.

## Drift measured on 2026-10-31

- Incorporated: `00c7613`, CADCraft 0.6.2 (`ecad/UPSTREAM.md`).
- Upstream `storytold/cadcraft` `main`: `7cee285` (2026-10-31), **CADCraft 0.15.1**. That is
  623 commits, 907 files, about +119k / -30k lines.
- **11 of the 12 seam code files changed upstream:** `engine/src/{lib,autosave}.rs`,
  `engine/src/cmd/{mod,file,edit,modify}.rs`, `ui-egui/src/{lib,chrome,closing,palettes}.rs`.
  `ext.rs` is ours and new. ECAD-owned `README.md`, `ci.yml` and many root `docs/` files changed too.
- Dependency moves:
  - egui, eframe, egui-wgpu and egui_kittest 0.36 → 0.37;
  - `rust-version` 1.90 → 1.95;
  - **new git dependencies on `storytold/craft-libs`**: `craft-window`, and `craft-update`
    (an update mechanism).

## Scope

1. Merge upstream `main` at a pinned commit into `upstream-sync/2026-10-31`, cut from `dev`. Use a
   real merge, not a rebase or a squash, so later syncs stay mergeable.
2. Resolve conflicts:
   - **Seam files:** re-apply each `ECAD-SEAM(owner)` change onto the new upstream code, keeping it
     minimal and generic.
   - **ECAD-owned files** (`UPSTREAM-PATCHES.md`, "ECAD-owned files outside `ecad/`"): keep ours.
   - **Upstream-only files:** take theirs.
3. Update the `ecad/` workspace for the dependency moves:
   - egui 0.37 API changes in `ecad-trial` and the `ecad-cadcraft` tests;
   - `rust-version`;
   - `ecad/Cargo.lock`.
4. `ecad/UPSTREAM.md`: the new `Commit:` line. `UPSTREAM-PATCHES.md`: re-check every row, and
   `cargo xtask patches` must pass against the new base.
5. Record each conflict and how it was resolved in the PR, seam by seam.

**Out of scope:**
- adopting or changing the deferred sync policy (PR #5, `ecad/docs/deferred-work.md`);
- deleting upstream clutter (R11);
- new ECAD features;
- fixing upstream bugs, which are reported, not patched.

## Questions for the owner before approval

1. Should `craft-update` (an update mechanism) be active in `ecad-trial`, disabled, or kept out of
   the dependency graph? The worker reports what it does: what it contacts and when.
2. Sync to `7cee285` or to the latest upstream release tag? The overseer recommends the newest
   release tag, if one is close to `main`, for stability.

## Acceptance criteria

1. `cargo xtask ci` (ECAD) and the root CADCraft CI are green on Linux and Windows.
2. `cargo xtask patches` passes. Every file differing from the new base outside `ecad/` is listed,
   and every code change carries `ECAD-SEAM(owner)`.
3. The full ECAD test suite passes unchanged in meaning. Any test edited only for an API change is
   listed in the PR with the reason.
4. `crates/engine/tests/ext_owner.rs`, `ext_commands.rs` and `crates/ui-egui/tests/toolsets_extra.rs`
   pass. Documents without an owner behave exactly as upstream.
5. The quick-start headless tests pass. `ecad/docs/quick-start.md` names the new CADCraft version and
   any changed steps.
6. No layering violation: `ecad-model`, `ecad-store` and `ecad-file` stay free of `cadcraft-*`.

## Needs owner check

The trial on Windows after the sync, using the quick-start worked example: drawing, convert, wire,
save, reopen, the debugger and the ECAD tab.
