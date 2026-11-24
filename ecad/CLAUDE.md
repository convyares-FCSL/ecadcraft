# ECAD workspace: instructions for agents

This folder holds all ECAD code as a Cargo workspace of its own. CADCraft (the parent directory's
`crates/`, `apps/`, `xtask/`) is the upstream drawing platform and is used by path dependency.

- **Reading (owner, 2026-11-12).** Read Part A of `../docs/document-architecture-proposal.md`
  (the fixed principles) and only the other sections your brief names; do not read the whole
  proposal. In `../docs/decisions.md` (one row per decision) look up the rows for your area rather
  than reading the whole log. The proposal's status: Part B is provisional; Part E is persistence
  and the live debugger (stages 1, 2a and 2b approved); libraries (stage 3), checkpoints and
  migration rewriting must not be implemented before their stage is approved. Postponed work is
  listed in `docs/deferred-work.md`; do not start it without the owner's approval.
- **Words (owner decision, 2026-11-08):** `docs/glossary.md` defines the words used in everything a
  person sees. A device's landing point for a conductor is a **connection point** (`-K01:A1`);
  one unit on the DIN rail is a **terminal block** (`-X01:1`), a tagged run of them a **terminal
  strip**. Code, ops, file keys and command names use the same words (`ConnectionPoint`,
  `AddConnectionPoint`, `addconnectionpoint`, `TerminalBlock`, `JumperBar`); "terminal" alone never
  appears in code.
- **Structure decisions (binding):** read `docs/design/decisions.md` before changing names, crates, layers or the
  source layout; the target tree is `docs/design/layout.md`. A change that would bend a decision goes to the owner
  first.
- **Docs are part of done (owner, 2026-11-22):** every commit updates the README, crate and module docs and
  comments it touches so they describe the code as it now is, in domain words. Same commit, no "docs later".
  Comments in `src/` and in built-in data (`defaults/*.toml`) carry no history (brief, design, lane, finding or
  question numbers, section marks, rulings, dates); history lives in git and `docs/`, and only READMEs link to
  `docs/`. `cargo xtask layers` enforces it.
- **`docs/quick-start.md` is the single maintained guide** for running and manually testing the
  app. Update it in the same PR as any change that affects its instructions, examples, expected
  results, limitations or the revision and environment it records. Keep automated checks,
  agent-observed runs and steps the owner has personally tried clearly apart, and never record a
  step as owner-tried unless the owner reported it. Other READMEs link to it rather than repeat
  it. The headless tests `the_quick_start_runs_headless` and
  `the_quick_start_debugger_example_runs_headless` follow its steps.
- The debugger is read-only: it reaches a project only through `ProjectHandle` reads under
  `try_lock`, copies what it shows and releases the lock before drawing, and runs replay, projection
  and file checks only on request, on copies. Keep it that way.
- **Branches and testing (owner decision, 2026-11-08).** ECAD is still being shaped, not at a
  production baseline, so development is fast and tests are deliberate; the full suite runs once,
  at the QA gate.
  - `main` is the released baseline. `dev` is integration: features land there when they work.
    `feature/<name>` is long-lived and holds one whole feature (cables, components, project
    manager…); `docs/features.md` lists them and which briefs belong to each.
  - A brief is a step inside a feature: it lands as commits on that feature's branch, not as a PR
    of its own. A feature reaches `dev` by **one PR** when it works; the owner may take it in more
    than one PR if it grows large.
  - **QA gate:** the PR from `dev` into `main`. It runs the full suite (CI's full jobs, screenshots,
    `cargo xtask bench`) and the owner's own click-through trial. Only the owner merges it.
    Before it, at the end of each major phase (e.g. before v0.2 goes to `main`), the overseer
    opens one **phase-end tidy-up** PR into `dev`: finished briefs' files in `docs/quick-start/`
    are folded into the guide or deleted (with the owner) and the index in `docs/quick-start.md`
    is brought up to date; lanes' `ecad.*` token sections move from their design specs into
    `docs/design/tokens.md`. Behaviour stays the same.
  - Never merge your own PR: green CI is not approval.
  - **Per-lane files (owner, 2026-11-13).** Lanes (briefs built at the same time on their feature
    branches) used to conflict at every merge on the same few lists. During a phase a lane adds
    its own files instead, and the shared lists are recombined only at the phase-end tidy-up:
    - **Quick start:** its own `docs/quick-start/<NNN>-<name>.md` (the brief's file name): what
      the brief changed and how to try it by clicking. Not `docs/quick-start.md`, whose index is
      updated only at the tidy-up; a change to a general section there (setup, running) is fine.
    - **Tests:** tests live in `tests/all/<feature>/` of each crate they test (the feature
      folder of the code they cover), one module per file. Still one test executable per crate.
    - **Commands:** a generic command is one `Entry` line in the `GENERIC` list of
      `ecad-engine/src/commands/mod.rs`; a module of session commands adds its `specs` to
      `commands::specs` and its `TYPED` to `typed_commands`. Its `mod` line in `lib.rs` stays where
      rustfmt sorts it. A command's parameters text (`help`, MCP's `list_commands`) is made from its
      params struct (`commands::help_of`): the struct's doc comment is the summary and its notes
      (alternative forms, units, refusals), each field's doc comment that field's help, and a closed
      set of words is an enum, so its words are listed. Never write the text by hand.
    - **Tokens:** new `ecad.*` canvas tokens go in a section of the lane's design spec
      (`docs/design/<NNN>.md`), not at the end of `docs/design/tokens.md`. The checked shell
      tables at the top of `tokens.md` (and their constants) are still edited in place: the
      doc-sync test reads that file.
    - Merge `dev` into the feature branch locally.
- **Checks are scoped to the change (owner decisions, 2026-11-02 and 2026-11-08).** This is Rust:
  what builds and passes clippy mostly works, so time goes on targeted tests, not repeated full runs.
  - **Worker (owner, 2026-11-12).** Before each commit: `cargo fmt --all -- --check` and the
    tests of the crates it touched since its last push, `cargo xtask test-changed` (its default
    base is the branch's upstream, so it tests what was not pushed yet; `--dry-run` names the
    crates; arguments after `--` go to the test executables, e.g. `-- cable_view` to filter). The
    crates that depend on them are not tested locally: CI runs their fast tests on the pull
    request into `dev` (`--with-dependents`). Before each
    **push** (not each commit): `cargo clippy --workspace --all-targets --all-features -- -D
    warnings`, which compiles every target, tests included. Never the whole workspace's tests.
    Documentation-only commits need no cargo at all.
  - **Test layout (brief 017).** Each crate has one integration test executable,
    `tests/all/main.rs`, whose modules are the former `tests/<name>.rs` files (shared helpers in
    `tests/all/common`), so tests are named `<name>::<test>` and `cargo test -p <crate> <name>`
    runs one file. A new test file is a new module there (under `tests/all/<feature>/`), never a
    new `tests/*.rs`, which would
    link egui and wgpu once more. ECAD's own crates build at `opt-level = 0` for dev and test, dependencies at 2.
  - **Slow set.** A test that takes more than about 2 s is marked `#[ignore = "slow: <why>"]`.
    Everyday runs (`cargo test`, `cargo xtask test-changed`) skip it. **Workers and testers never
    run `--include-ignored`** (owner, 2026-11-12): the slow set runs only at the dev→main QA gate
    (`cargo xtask ci`), or when the owner calls for it after massive changes. Child-process
    helpers are named `child_*`, keep their own `#[ignore = "run by … in a child process"]`
    reasons, and do nothing unless the test that starts them set `ECAD_TEST_CHILD`, so an
    `--include-ignored` run is always safe. A new one follows the same pattern.
  - **Tester:** only where a mistake is expensive: a `.ecad` format or op change, movement and
    `translate.rs`, the store and locking, or anything the owner asks for. It runs its own
    acceptance tests and the fast tests of the crates the change touched: no slow set, and no
    whole-workspace run, also when shared crates changed. A re-test runs only what failed.
  - **Reviewer:** once per feature, on the PR into `dev`, and on any brief the tester would cover.
    Reads; runs cargo only for `cargo xtask patches` or to confirm a specific finding.
  - **CI:** pull requests into `dev` run the quick check on Linux: fmt, clippy with all targets
    (so every test must still compile), and the fast tests of the crates the pull request changed
    and of every crate that depends on them (`cargo xtask test-changed --base origin/dev
    --with-dependents`; owner, 2026-11-12). The full suite (what `cargo xtask ci` runs, with the
    slow set, root CADCraft, Linux and Windows) runs on pull requests into `main`, on
    pushes to `main` and by hand (`workflow_dispatch`), with the large bench; CI splits it into
    `cargo xtask lint` and sharded cargo-nextest jobs (`.config/nextest.toml`), and Windows leaves
    out the in-memory slow tests named there.
- **Builds (owner decision, 2026-11-08, replaces the shared folder of 2026-11-01):** each checkout
  builds into **its own** folder, so builds running at the same time never mix: leave
  `CARGO_TARGET_DIR` unset, and cargo uses that checkout's own `ecad/target` (git-ignored). The
  CADCraft root workspace likewise uses its own `target`. Always set
  `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` (incremental builds stay on: each
  checkout's own folder makes them safe, and they are faster). A worker and the owner trying the same feature
  share that feature folder's build, so they never build it at the same time: the worker says when
  it is done building. A tester's worktree under `_tmp` builds into its own `target` and is deleted
  with it. The old shared `C:/dev/ecad-target` mixed a stale `ecad-model` into builds from two
  worktrees (2026-11-08); it is no longer used.
  **Cleaning (owner, 2026-11-12):** `cargo xtask clean-builds --root C:\dev\ecad-features` deletes
  `ecad/target` and `target` in every feature folder and in `_tmp`, skipping any folder a cargo or
  rustc is using, removes worktrees whose branch is merged into `origin/dev` (never one with
  uncommitted changes), and prints the space freed; `--dry-run` only says what it would do. Task
  Scheduler runs it daily at 03:00 on the owner's machine, so a build folder may be gone in the
  morning: the next build makes it again.
- **Folders (owner decision, 2026-11-08).** Nothing new goes directly in `C:\dev`:
  - `C:\dev\ecadcraft` is the main checkout, on `dev`. The overseer works here.
  - `C:\dev\ecad-features\<feature>` is the one worktree of a feature branch
    (`feature/<feature>`). It is removed when the feature is merged into `dev`.
  - `C:\dev\ecad-features\_tmp\<agent>-<what>` is for anything temporary: a tester's worktree, a
    kept benchmark project, a private build folder, logs. Whoever makes it deletes it when done;
    anything left in `_tmp` may be deleted at any time.
  - `C:\dev\ecad-projects` (the owner's projects) stays.

```sh
# worker (from ecad/ in its feature folder), CARGO_TARGET_DIR unset
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo fmt --all -- --check        # before each commit
cargo xtask test-changed          # before each commit: fast tests of the crates not pushed yet
cargo clippy --workspace --all-targets --all-features -- -D warnings   # before each push
cargo xtask patches               # only when files outside ecad/ changed
```

  When a change touches files outside `ecad/`, the worker also builds and clippies CADCraft's
  root workspace (in its own `target`), and the tester runs the affected
  CADCraft crates' tests.
- **Layers (design 065, `docs/design/crates.md` §1 and §4).** **Core** (what a project is and
  means), in this order: `ecad-model` (definitions), `ecad-store`, `ecad-file`, `ecad-electrical`
  (what things mean electrically), `ecad-drawing-style` (how they look). **Engine** (the link to CADCraft): `ecad-engine`. **UI kit**: `ecad-widgets`
  (with its `tokens` module). **App**: `ecad-app`. **Programs** (one `main` each): `ecad-desktop` and
  `ecad-mcp` (headless use goes through `ecad-mcp`). **Dev** (never shipped): `ecad-bench`, `ecad-inspector` and `xtask` under `dev/`.
  A crate uses only layers above it; `cargo xtask layers` enforces the table.
- No Core crate depends on a `cadcraft-*` crate. Only `ecad-engine` touches the engine; App and
  Programs reach it through `ecad_engine::cad`.
- **ECAD is desktop-only (owner ruling, 2026-11-15):** no web or wasm product, so no ECAD crate
  has to build for wasm and there is no `cargo xtask wasm`. CADCraft's own wasm build is not
  affected.
- **Derived views scale with what changed (owner decision, 2026-11-08; revised 2026-11-21).** Until
  release, a view, list or index gets an **update map** only where the large-tier bench shows a full
  rebuild is noticeable: over about **5 ms per edit on the large project** (the Devices list at
  24–42 ms on 8 Nov is the example that justifies one). Otherwise it rebuilds from scratch after a
  commit, and no incremental code is kept for it. Incremental code comes back where it earns its
  keep; a solid foundation comes first. Only the page on screen redraws; other open pages are marked
  stale and redraw when brought to the front. The loading rule is unchanged: a project opens on
  demand, and opening does not grow with history. `cargo xtask bench` (large tier) is the evidence
  for each kept update map.
- **Benches (owner, 2026-11-12).** While building, bench on the **medium** project. Run the large
  tier once, at the end of the brief, and only for the operations the brief sets targets for. The
  full `cargo xtask bench` runs at the QA gate. Print and PDF benches (Print Book, release builds)
  use a project of **at most 25 pages**, never the 100- or 500-page tiers.
  **Update maps (owner, 2026-11-21).** Where a view keeps one (rule above), the map says which
  effects change what it shows, what it does for each, and that every other effect does nothing.
  No refresh buttons: views stay live.
- Changing any file outside `ecad/` is an upstream patch: list it in `UPSTREAM-PATCHES.md` and mark
  code with `// ECAD-SEAM(owner):`. Keep ECAD semantics out of upstream crates.
- Rust only for product code. **Exception (owner decisions, 2026-10-12 and 2026-11-12):** developer
  tools under `ecad/dev/` may be written in HTML, CSS and JavaScript (for example the read-only
  inspector page, `ecad/dev/ecad-inspector/page/index.html`). They ship with no product, run no server by default, never
  mutate a project, and escape every value they display. Anything an end user runs stays Rust.
- Never-crash rules apply: no `unwrap`/`expect`/`panic!` in non-test code, no `unsafe`, errors
  through each crate's error type.
- Style: `rustfmt` at 120 columns; crate `//!` docs say what the crate excludes; public items
  document `# Semantics`, `# Errors`, `# Example` where useful; contract tests are table-driven in
  `tests/all/<area>_contracts.rs`.
- **Components (owner, 2026-11-15).** Panels and dialogs use only `ecad_widgets` components for values
  (`docs/design/components.md`): `control::TextEntry` (text, or a number with its unit),
  `control::Select`, `prim::Segmented`, `control::Toggle`, `control::ValueBox` (a value with its
  hint), `control::read_text` / `prim::read_value` (read from the drawing), `control::link`,
  `panel::status_pill`, `panel::section_header`, `prim::Button`. No hand-rolled `egui` labels or
  inputs for values. Anything the person can change looks editable at a glance; a read value never
  looks like an input. `cargo xtask layers` fails on a new raw `egui::TextEdit` or `ComboBox` in an
  ECAD UI crate; the files that still have them are listed (`RAW_CONTROL_FILES`, no counts) until
  they are converted. A
  missing component is added to `ecad_widgets` and its gallery (`showcase::components`), not drawn in the
  panel.
- Versions: product `0.3.0-dev` (`dev` carries the next release with `-dev`; the QA gate drops
  the suffix and the tag follows), release tags `ecad-v…`, CADCraft commit in `UPSTREAM.md`, format
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
| `worker` | Implements one brief on the feature's branch | Merge, push to `dev`/`main`, judge its own work done |
| `tester` | Tests the branch against the brief in its own worktree; headless UI tests and screenshots | Change production code |
| `reviewer` | Read-only review of the diff | Edit anything |

Loop for each brief in `docs/briefs/` (owner decisions, 2026-11-08 and 2026-11-12):
1. **Designer**, if the brief has UI.
2. **Worker**, with the brief (and design spec), on the feature's branch. It commits, runs its own
   scoped checks (above) and pushes; pushing a feature branch needs no further approval.
3. **Tester**, only for a brief in the tester's list above (with only the brief, the design spec
   and the branch name, in a separate worktree; never the worker's report).
4. **Draft PR:** the overseer opens a draft PR from `feature/<name>` into `dev` and runs
   `reviewer` on it.
5. **Owner:** tries it (click-only steps in the quick start), marks the PR ready and merges.
   Agents **never merge**.

**Fix rounds (one rule for tester and reviewer findings):** the worker gets one round per set of
findings, and the re-test or re-review covers only what failed. After three failed rounds, stop and
report to the owner.

At a milestone, the owner asks for the QA gate: first the phase-end tidy-up PR into `dev` (see
"Branches and testing"), then a PR from `dev` into `main`, full suite, screenshots, the full
bench, and the owner's trial.

**Agent access is built in (owner decisions, 2026-11-07, brief 011, and 2026-11-23).** Generic
commands (the engine's `GENERIC` list, each with its `Access`) are covered by one table,
`ecad/apps/ecad-mcp/tests/all/generic_commands_contracts.rs`: each command that may edit has a row
of parameters that change a small project; with writes off they are refused in the gate's words
and record nothing, with writes on they record exactly one transaction by
`Author::Agent(<client name>)`. A new generic command fails `every_generic_edit_command_has_a_row`
until its row is added. Every brief that adds or changes a session command, or a generic command's
own edge (a form that only reads, a refusal in its own words), includes in its acceptance criteria
an MCP contract test: it drives the command through `ecad-mcp`'s `execute` tool (as
`ecad/apps/ecad-mcp/tests/all/mcp_contracts.rs` does) and checks the transaction it records, its
author, and that the command is refused with writes off. The worker writes it with the command; the
tester covers it like any other criterion. A brief without one is not ready for approval.

Gates where the overseer stops and asks the owner: starting any brief not marked `Approved`; any
change outside `ecad/` that the brief does not approve; any change to a fixed principle (proposal
Part A) or a recorded decision; pushing to `dev` or `main`; anything the tester lists as "needs
owner check" on native Windows. Marking a PR ready is the owner's own step.

The architect (Carl, with Claude chat in the ECAD Project) writes and approves briefs. The overseer
may draft briefs, marked `Status: Draft`, but does not start them.
