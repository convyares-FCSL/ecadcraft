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
- Before every commit run `cargo xtask ci` from `ecad/` (fmt, clippy `-D warnings`, tests, layers,
  upstream patch guard, wasm). Build with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0` to keep
  the target directory near 6 GB.
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
3. Run `tester` with only the brief, the design spec and the branch name, in a separate worktree;
   never pass it the worker's report.
4. On FAIL, send the tester's findings back to `worker`; repeat. After three failed rounds, stop and
   report to the owner.
5. On PASS, run `reviewer`. Serious findings go back to `worker`.
6. Ask the owner before pushing; open the PR into `dev` with the brief, the test report and the
   review summary. **Never merge.** The owner merges.

Gates where the overseer stops and asks the owner: starting any brief not marked `Approved`; any
change outside `ecad/`; any change to a fixed principle (proposal Part A) or a recorded decision;
pushing a branch; anything the tester lists as "needs owner check" on native Windows.

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
