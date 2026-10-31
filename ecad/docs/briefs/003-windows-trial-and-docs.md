# Brief 003: The trial on native Windows, and document corrections

Status: Draft (overseer, 2026-10-31)
Branch: `ecad/windows-trial`
Designer: no · Size: M
Gate: part B changes files outside `ecad/` and recorded decisions, so it needs explicit owner
approval of each correction.

## Goal

The owner can build, run and manually try `ecad-trial` on native Windows by following one guide,
and the project's documents agree with the code and the 2026-10-31 decisions.

## Part A: the trial on Windows (in `ecad/`)

Finding ids refer to `ecad/docs/review-2026-10-31.md`.

1. **R-S3 Home folder.**
   - Resolve `~` with `std::env::home_dir()`, accept both `~/` and `~\`, and build every path with
     `Path::join`, in `apps/ecad-trial/src/lib.rs` and `crates/ecad-cadcraft/src/file.rs`.
   - Never create a folder literally named `~`.
   - The default folder follows owner question 2.
2. **Arguments:** `main.rs` uses `args_os()` (minor 1). Move argument parsing into the library so it
   can be tested.
3. **Own file:** compare canonical paths, not text (minor 3).
4. **Wording:** platform-neutral usage and dialog text (minor 2).
5. **Console window:** add `windows_subsystem = "windows"` for release builds only, as upstream
   `apps/cadcraft` does (minor 11).
6. **Quick start:**
   - add a native Windows section (toolchain, build, run, where projects are saved, opening the
     inspector from a Windows path) beside the WSL one;
   - correct the "everything in sections 4 to 7" claim (D15);
   - record the environment and revision.

## Part B: document corrections (outside `ecad/`, owner approval per item)

Item numbers refer to the review's §4.

- `docs/decisions.md`: add rows, and never edit old rows:
  - D1 and D2: the seams and crate names were superseded by proposal R1 and §8 as built;
  - D11: native Windows and Linux are supported, superseding E14 (owner decision 2026-10-31);
  - D14: the Phase 0 assessment was overtaken by the approved proposal.
- `docs/document-architecture-proposal.md`:
  - D4: status header, E10 and E12 show stage 2b as merged;
  - D3: mark the §8 layout as target, not current, or list the crates as built;
  - D10: follow brief 002's outcome;
  - add a revision-history row.
- `docs/concept-spec.md`: a superseded note at the top pointing to the foundations note and the
  proposal for D5-D9. No rewrite.
- `ecad/UPSTREAM.md` (D12): drop `TEST_STORE_FORMAT`.

**Out of scope:** R11 housekeeping (rewriting the root `CLAUDE.md`/`AGENTS.md`, disabling inherited
workflows) and PR #5's upstream sync policy, which stays deferred.

## Acceptance criteria

1. A table-driven contract test of path input (`tests/paths_contracts.rs`). It covers:
   - `HOME` unset with the profile known;
   - `~/` and `~\`;
   - drive letters, backslashes, spaces, non-ASCII, UNC;
   - a case-variant spelling of the bound file;
   - a reserved name (`CON.ecad`) refused with a message;
   - no literal `~` folder ever created.
2. An argument-parsing test covers a project path, `--snapshots`, both, and usage errors, including a
   non-UTF-8 argument on Unix.
3. `cargo xtask ci` is green on both runners. The quick-start headless tests still pass.
4. `the_quick_start_trial_runs_headless` saves through the dialog with a `~/...` path under a
   temporary home, and asserts the banner text and tab title the guide quotes (test audit, item 7).
5. Every document change in part B is listed in the PR, and each was approved by the owner.

## Needs owner check (native Windows)

Review §3, "Needs owner check", items 1-5 and 10-12.
