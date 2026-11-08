# Brief 017: Tests built for rapid development

Status: Approved (owner, 2026-11-08: "Approve 017")
Branch: `feature/fast-tests` (worktree `C:\dev\ecad-features\fast-tests`)
Designer: no · Size: M
Depends on: nothing; runs before any new feature (owner decision, 2026-11-08, `docs/features.md`).

## Goal

Running the tests of what changed takes seconds to a minute, not ten. ECAD is Rust and is still
being shaped: what builds and passes clippy mostly works, so tests must be cheap enough that a
worker runs the relevant ones on every commit without thinking about it.

## Evidence (CI log of PR #34, Linux, 2026-11-08)

| Step | Cold | Warm |
|---|---|---|
| `cargo clippy --all-targets --all-features` | 3 m 20 s | 58 s |
| Building the tests (`cargo test --no-run`) | 7 m 51 s | over 5 min |
| **Running** every ECAD test | 26 s | — |

- About 58 test files across the workspace, and each is its own test executable:
  - `ecad-cadcraft`: 20 files, about 12,900 lines;
  - `ecad-trial`: 18 files;
  - `ecad-file`: 7;
  - `ecad-bench`: 5;
  - `ecad-model`: 4;
  - `ecad-mcp`: 3;
  - `ecad-store`: 1.
- Every executable that depends on `ecad-cadcraft` or `ecad-trial` links the whole egui and wgpu stack, so most of the time goes on linking, not compiling.
- The `dev` profile builds ECAD's own crates at `opt-level = 1` and dependencies at `opt-level = 2` (`ecad/Cargo.toml`). The tests are optimised builds, which is slower again.

## Scope

1. **One test executable per crate.** Each crate's `tests/*.rs` files become modules of a single
   integration test target (for example `tests/all/main.rs` with `mod cable_look_contracts;`…),
   sharing `tests/common`. Test names and assertions are unchanged; only the layout moves.
   Filtering by module still works (`cargo test -p ecad-cadcraft cable_look`).
2. **Measure the profile choice.** Time `cargo test -p ecad-cadcraft --no-run` and the run, warm,
   with the current profile and with ECAD's own crates at `opt-level = 0` (dependencies kept at 2).
   Keep whichever has the lower build plus run time for a typical change; report both.
3. **A fast default and a slow set.** Mark the slow tests (the 400-edit oracle probe, the big
   generator and bench tests, and anything over about 2 s) `#[ignore = "slow: …"]`. They then run
   at the QA gate with `--include-ignored`. The worker's scoped check and CI's full suite say which
   they run.
4. **One command for "test what I touched".** For example `cargo xtask test-changed`: from
   `git diff` against `dev`, it runs `cargo test -p` for the changed crates and their dependents.
   It's documented in `ecad/CLAUDE.md` as the worker's default check.
5. **Docs:** `ecad/CLAUDE.md` ("Checks are scoped to the change") and `docs/quick-start.md` where
   they name test commands. CI's full suite uses `--include-ignored`.

Out of scope: changing what any test asserts, deleting tests (except exact duplicates, listed for
the owner), CADCraft's root workspace tests, and the CI quick check (it stays fmt and clippy).

## Acceptance criteria

1. Every test that ran before still runs, either in the fast default or in the slow set, with the
   same name and result. A before-and-after list of test names is attached to the PR, and the
   counts match.
2. Warm, on the owner's machine:
   - after a one-line change in `ecad-cadcraft`, `cargo test -p ecad-cadcraft` (build plus run)
     takes under 60 s;
   - after the same change, the whole fast default set takes under 2 min.

   Before and after figures are in the PR.
3. CI's full suite on a PR into `main` runs the slow set and stays green.
4. `cargo xtask test-changed` picks the right crates for a change in `ecad-model` (everything that
   depends on it) and for a change in `ecad-trial` (only itself).

## Needs owner check

- Approve this brief, and the speed targets in criterion 2.
