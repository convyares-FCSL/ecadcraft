# ECAD workspace: instructions for agents

This folder holds all ECAD code as a Cargo workspace of its own. CADCraft (the parent directory's
`crates/`, `apps/`, `xtask/`) is the upstream drawing platform and is used by path dependency.

- Read `../docs/document-architecture-proposal.md` (principles in Part A are fixed; Part B is
  provisional; Part E is persistence and the live debugger: stage 1, records in memory,
  stage 2a, project files on disk, and stage 2b, the read-only debugger panel, are approved;
  libraries (stage 3), checkpoints and migration rewriting must not be implemented before their
  stage is approved) and `../docs/decisions.md` first. Postponed work is listed in
  `docs/deferred-work.md`; do not start it without the owner's approval.
- **`docs/quick-start.md` is the single maintained guide** for running and manually testing the
  app. Update it in the same PR as any change that affects its instructions, examples, expected
  results, limitations or the revision and environment it records. Keep automated checks,
  agent-observed runs and steps the owner has personally tried clearly apart, and never record a
  step as owner-tried unless the owner reported it. Other READMEs link to it rather than repeat
  it. The headless tests `the_quick_start_trial_runs_headless` and
  `the_quick_start_debugger_example_runs_headless` follow its steps.
- The debugger is read-only: it reaches a project only through `ProjectHandle` reads under
  `try_lock`, copies what it shows and releases the lock before drawing, and runs replay, projection
  and file checks only on request, on copies. Keep it that way.
- Branch from `dev`; every change arrives as a PR into `dev`. Never merge your own PR: green CI is
  not approval.
- **Checks are scoped to the change (owner decision, 2026-11-02).** This is Rust: what builds and
  passes clippy mostly works, so time goes on targeted tests, not repeated full runs.
  - **Worker, before each commit:** it builds and is clean: `cargo fmt --all -- --check` and
    `cargo clippy --workspace --all-targets --all-features -- -D warnings` (which compiles every
    target, tests included). Workers run no test suites, except to reproduce a failure they are
    fixing. Documentation-only commits need no cargo at all.
  - **Tester:** runs the tests that suit the scope of the change: its own new acceptance tests,
    then the tests of the crates changed (`cargo test -p <crate>`) and their dependents. The whole
    workspace only for a change that touches shared code (`ecad-model`, `ecad-file`, `ecad-store`)
    or CADCraft. A re-test after a fix round runs only what failed plus the crate that changed.
  - **Reviewer:** reads; runs cargo only for `cargo xtask patches` or to confirm a specific finding.
  - **CI** runs everything (`cargo xtask ci` with wasm, plain `cargo test --workspace`, root
    CADCraft, Linux and Windows) on every pull request. Run wasm locally only when a change could
    affect it (dependencies, `cfg(target_arch)`, platform code in `ecad-model`/`ecad-store`/`ecad-file`).
- **Builds (owner decision, 2026-11-01):** to stop every agent compiling from scratch, agents that
  run one after another share one build folder, even from their own git worktree:
  `CARGO_TARGET_DIR=C:/dev/ecad-target` for the ECAD workspace and `C:/dev/ecad-target-root` for
  CADCraft's root workspace, always with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. Only an
  agent that builds while another agent is building at the same time uses its own folder
  (`C:/dev/ecad-target-<agent>`), and deletes it when done. Never delete the shared folders.

```sh
# worker (from ecad/), with CARGO_TARGET_DIR=C:/dev/ecad-target
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo xtask patches               # only when files outside ecad/ changed
```

  When a change touches files outside `ecad/`, the worker also builds and clippies CADCraft's
  root workspace (`CARGO_TARGET_DIR=C:/dev/ecad-target-root`), and the tester runs the affected
  CADCraft crates' tests.
- `ecad-model`, `ecad-store` and `ecad-file` must never depend on a `cadcraft-*` crate. Only
  `ecad-cadcraft` touches the engine.
- Changing any file outside `ecad/` is an upstream patch: list it in `UPSTREAM-PATCHES.md` and mark
  code with `// ECAD-SEAM(owner):`. Keep ECAD semantics out of upstream crates.
- Rust only for product code. **Exception (owner decision, 2026-10-12):** developer tools under
  `ecad/tools/` may be written in HTML, CSS and JavaScript (for example the read-only inspector,
  `ecad/tools/inspector/index.html`). They ship with no product, run no server by default, never
  mutate a project, and escape every value they display. Anything an end user runs stays Rust.
- Never-crash rules apply: no `unwrap`/`expect`/`panic!` in non-test code, no `unsafe`, errors
  through each crate's error type.
- Style: `rustfmt` at 120 columns; crate `//!` docs say what the crate excludes; public items
  document `# Semantics`, `# Errors`, `# Example` where useful; contract tests are table-driven in
  `tests/<area>_contracts.rs`.
- Versions: product `0.1.0-dev`, release tags `ecad-v…`, CADCraft commit in `UPSTREAM.md`, format
  versions independent of the product version.

## Working home and platforms (owner decision, 2026-10-31)

- Claude Code runs from `ecad/` in the Windows clone `C:\dev\ecadcraft`, branch `dev`. The parent
  CADCraft folder is readable through `.claude/settings.json`. The root `CLAUDE.md` and `AGENTS.md`
  are upstream CADCraft's: where they conflict with this file (for example "push to `main`"), this
  file wins for all ECAD work.
- ECAD must build, test and run natively on **Windows and Linux**. Linux is checked by CI and by a
  pull-only WSL clone; never edit in two clones. `core.autocrlf` is `false`; files are LF.
- `private/` holds the owner's local chat history and handover notes. It is git-ignored. Read it for
  context; never copy its contents into committed files, and never commit anything from it.

## Operating model: overseer and sub-agents

The main Claude Code session is the **overseer**. It plans and coordinates; it does not write
production code itself. Sub-agents are defined in `.claude/agents/`:

| Agent | Does | Never |
| --- | --- | --- |
| `designer` | UI and interaction spec for briefs with a user-facing part, in `docs/design/` | Write Rust |
| `worker` | Implements one brief on its own branch | Merge, push to `dev`/`main`, judge its own work done |
| `tester` | Tests the branch against the brief in its own worktree; headless UI tests and screenshots | Change production code |
| `reviewer` | Read-only review of the diff | Edit anything |

Loop for each brief in `docs/briefs/`:
1. If the brief has UI, run `designer` first.
2. Run `worker` with the brief (and design spec).
3. Push the branch and open a **draft** PR into `dev` straight away, so CI (Linux and Windows) runs
   while the branch is checked (owner decision, 2026-11-02: pushing a work branch to a draft PR
   needs no further approval).
4. Run `tester` (with only the brief, the design spec and the branch name, in a separate worktree;
   never the worker's report) and `reviewer` **at the same time**.
5. Send the tester's failures and the reviewer's serious findings back to `worker` in one round;
   re-test only what failed. After three failed rounds, stop and report to the owner.
6. When the tester passes, the review is clean and CI is green, ask the owner before marking the PR
   ready, with the brief, the test report and the review summary in it. **Never merge.** The owner
   merges.

Gates where the overseer stops and asks the owner: starting any brief not marked `Approved`; any
change outside `ecad/` that the brief does not approve; any change to a fixed principle (proposal
Part A) or a recorded decision; marking a PR ready for review; pushing to `dev` or `main`; anything
the tester lists as "needs owner check" on native Windows.

The architect (Carl, with Claude chat in the ECAD Project) writes and approves briefs. The overseer
may draft briefs, marked `Status: Draft`, but does not start them.

## Priority order (owner decision, 2026-10-31)

1. Baseline review and plan (this session's first job; see `docs/briefs/000-baseline-review.md`).
2. Windows-native readiness: Windows CI job, `ecad-store` Windows equivalents, and verified save and
   locking on Windows.
3. **Wires to cable** as the first visible test of the model: slash gesture creating a cable from
   existing conductors, and a traditional / fan view switch that is presentation only.
4. Stage 3: libraries (`.ecadlib`).
5. Everything else, in an order the owner approves.
