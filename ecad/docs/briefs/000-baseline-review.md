# Brief 000: Baseline review and plan

Status: Approved (owner, 2026-10-31)
Branch: `ecad/baseline-review`

## Goal

Establish an accurate, shared picture of where ECAD stands, now that development has moved to the
Windows clone, and propose the plan for the next briefs. No production code changes.

## Inputs

- `ecad/CLAUDE.md` (this operating model), `docs/ecad-architecture-foundations.md`,
  `docs/document-architecture-proposal.md`, `docs/decisions.md`, `docs/concept-spec.md`,
  `ecad/docs/*`, `ecad/UPSTREAM.md`, `ecad/UPSTREAM-PATCHES.md`.
- Owner-local context in `ecad/private/` (read only; do not quote into committed files).

## Tasks

1. **Windows build check (overseer, natively on Windows):** `rustc`/`cargo` versions; root
   `cargo build --workspace`; from `ecad/`: `cargo xtask ci`, plain `cargo test --workspace`, and
   whether `cargo run --release -p ecad-trial` opens a window. Record exact results and failures.
2. **Independent code review (`reviewer`):** the whole `ecad/` workspace and every upstream seam,
   for architecture fit, Windows risks, never-crash rules and test gaps.
3. **Independent test audit (`tester`):** which behaviours in `docs/quick-start.md` and the proposal
   have automated tests, which do not, and which can only be checked by the owner on screen.
4. **Reconcile documents:** list every place where `docs/concept-spec.md`, the proposal,
   `decisions.md` and the code disagree, and which one is current.
5. **Plan:** propose briefs, in the priority order in `ecad/CLAUDE.md`, each with goal, scope,
   acceptance criteria, whether it needs the designer, and rough size. Write each as
   `docs/briefs/NNN-<name>.md` with `Status: Draft`.

## Deliverables

- `docs/review-2026-10-31.md`: findings from tasks 1–4, most important first.
- Draft briefs 001 onwards.
- Small housekeeping commit allowed on this branch only: the `.gitignore` entry for `private/`, this
  brief, the agent files and the `CLAUDE.md` update already in the working tree.

## Stop point

Stop after the deliverables. Ask before pushing. Do not start any draft brief.
