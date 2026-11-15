# Brief 067: the crate split

Status: Approved (owner, 2026-11-15: design 065, `docs/design/crates.md`).
Branch: `feature/crates`, from `refactor/0.2.3` at `9a27bd5a`.
Designer: no · Tester: yes (step 1c, and steps 4 and 6) · Size: L.

## The brief

The approved design `docs/design/crates.md` is the brief. Follow its §5 step order; §1–§4 and §6
give the details. Behaviour does not change; test changes are limited to paths. If a cut does not
work as designed, or anything would change the `.ecad` format or add an op, stop and report.

## Overrides to §5 (owner process for 0.2.3, 2026-11-15)

- **Precondition.** Lanes 060–063 are merged into `refactor/0.2.3`, not `dev`. There are no PRs.
  When the lane is green, the overseer merges `feature/crates` into `refactor/0.2.3`. If that merge
  conflicts, the worker merges `origin/refactor/0.2.3` into `feature/crates` and resolves it there.
- **Step 10, CI.** `.github/workflows/*` is not edited (house rule 11). CI's ECAD lint job runs
  `cargo xtask lint`, so removing the wasm step from `xtask lint`/`ci` and removing
  `cargo xtask wasm` is enough. The stale `targets: wasm32-unknown-unknown` line and the step name
  in `ci.yml` are a separate CI task.
- **Step 11** (drop the re-exports) is skipped; it is left for the phase-end tidy-up.
- **Commits.** One commit per step, `067 step N: …`, so `git bisect` lands on a step. The branch is
  pushed after each green step.
- **Tester.** After step 1c the worker reports, and the tester covers 1c while steps 2–6 go on.
  The worker reports again after step 6 and at the end.

## Checks per step

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D
warnings`, `cargo xtask layers` and `cargo xtask test-changed`. Never the whole workspace or the
slow set.
