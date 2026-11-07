# Brief 013: In-place replay and incremental validation

Status: Approved (owner, 2026-11-07, with the rollback-journal approach of scope 2). Constraint
on multi-op transactions added by the owner, 2026-11-07.
Branch: `ecad/incremental-validation`
Designer: no (no UI) · Size: M
Depends on: nothing for the code (branched from `dev`). Acceptance criterion 5 is measured once
012 (PR #30) is merged and this branch is rebased on it, so the benchmark exists.

## Goal

Applying a transaction costs in proportion to what it touches, not to the size of the project:
when a file opens (replay), on every live edit, and on undo and redo. Every guarantee stays as it
is: every state in history is valid, a damaged file opens read-only at its last good transaction,
and the error names the first bad transaction.

## What exists today (and why it is slow)

- `ecad-model/src/ledger.rs`:
  - `apply_all` clones the whole `Document`, applies the transaction's ops to the clone, then runs
    the full `Document::check` (`doc.rs:395`). That makes the work per transaction proportional to
    the document's size, and replay quadratic in history: a file of N transactions does roughly
    N(N+1)/2 object checks.
  - `apply_all` is called from replay (`replay`, `replay_prefix`, `from_checkpoint`), `state_at`
    (debugger history and Verify replay), and `append` (every commit, undo and redo).
- The clone exists because `Op::apply` is not all-or-nothing: `AddPage` inserts the page before it
  rejects a bad index (`op.rs:246-251`). A failed transaction must leave the document unchanged, so
  the ledger works on a copy.
- The full check runs once per transaction, after all its ops. **States between the ops of one
  transaction are allowed to break invariants**, as long as the transaction's end state is valid.
  Some commands rely on this, for example remove-then-add pairs such as renaming a page.
- Smaller hotspots of the same kind, found during review:
  - `undo_label` and related functions rebuild the undo stacks from the whole log on each call
    (`stacks(&self.log)`);
  - `Document::cables_of` scans every cable.

## Scope

1. **Every op is atomic.** `Op::apply` checks all its preconditions before it mutates anything, so
   an op that returns `Err` leaves the document untouched. Every op type is covered, each by a test.
2. **Ops apply in place, in order, with a rollback journal.** The ledger applies a transaction's
   ops directly to the live document, one at a time, recording each applied op. If an op fails,
   or the end-of-transaction check fails, the ledger applies the recorded ops' inverses in reverse
   (`Op::inverse` already exists), and the document is back exactly as it was before the
   transaction. There is no clone.
   - **Dependencies inside a transaction** (owner constraint): because the ops run in order on the
     live state, op 2 sees what op 1 created, modified or removed, exactly as it does today. No
     separate look-ahead model is needed.
   - **States inside a transaction** (owner constraint): invariants are not checked between ops,
     only once at the end of the transaction, as today. An intermediate state that the final op
     resolves is still accepted.
   - Rollback must be infallible: the inverse of an applied op always succeeds on the state that op
     produced. That needs a test per op type, and a debug assertion in the ledger.
   - **The alternative** the owner allowed: a check-only pass that keeps a scratch delta of touched
     IDs, followed by an infallible apply. The worker may propose it instead, with the reason, but
     the journal is preferred: it reuses `Op::inverse`, and it never keeps a second model of the
     document that could drift from `Op::apply`.
3. **Checks are scoped to what the transaction touched.** While ops apply, the ledger collects a
   touched set: the IDs created, modified or removed, plus the objects whose invariants depend on
   them. For example, a removed terminal touches the conductors attached to it and the
   representations of its component; a page removal touches the page order and the items on that
   page.
   - `Document::check_touched(&touched)` checks every invariant that could have changed for those
     objects.
   - Deriving the dependents is part of the work: list each invariant (DOC-02 to DOC-07 and any
     unnumbered ones in `check`) and state which objects it relates. Write that table into the
     module's docs.
4. **Equivalence with the full check, as the safety net.**
   - In test builds, and behind a runtime switch (`ECAD_VERIFY_INVARIANTS=1`, which the debugger's
     **Verify replay** turns on for its run), the ledger also runs the full `check` after every
     transaction and asserts that it agrees with `check_touched`: both pass, or both fail.
   - A property test runs random op sequences, both valid and deliberately invalid, through both
     checks.
   - The existing replay, file and golden tests run with the switch on.
5. **Replay keeps every guarantee.**
   - Production open checks every transaction incrementally; it never switches to "check only at
     the end".
   - `replay_prefix` still stops at the first bad transaction, with the document at the last good
     state (the rollback leaves it there) and the same error wording.
6. **The smaller hotspots:**
   - keep the undo and redo stacks with the ledger instead of rebuilding them from the log on each
     call;
   - add indexes for the queries that live editing calls per object (conductor to cables,
     terminal to conductors), kept up to date by `Op::apply`, and checked by the full check in
     verify mode.

Out of scope: checkpoints and any change to the file format or ops (no `.ecad` change; the same
transactions are recorded byte for byte), the file container discussion, CADCraft projection
speed.

## Acceptance criteria

1. Every existing test passes unchanged, including the golden file and fault-injection tests.
   Transactions recorded for the same commands are byte-identical to `dev`.
2. Every op type: atomic on failure, and inverse after apply returns the exact previous document
   (tests).
3. The property test of `check_touched` against the full `check` passes over at least 10,000
   random sequences, including invalid ones. Verify mode asserts agreement on every transaction in
   the existing replay tests.
4. A multi-op transaction whose intermediate state breaks an invariant, and whose end state is
   valid, still commits. One whose op 2 depends on an ID created by op 1 commits. One that fails at
   op 3 leaves the document byte-identical to before (tests).
5. `cargo xtask bench --tier small,medium --compare <012 baseline>`:
   - open time grows roughly linearly with the transaction count. Churned medium opens in about
     the multiple of lean medium that its transaction count is, not that multiple squared;
   - `edit ms` on medium and large no longer grows with the project's size.
   The worker reports the before and after tables. No fixed number is promised, since that is
   machine-dependent.
6. A damaged file still opens read-only at its last good transaction with the same message
   (existing tests).

## Needs owner check

- Approve the journal approach (scope 2) over the scratch-delta alternative, or say to evaluate
  both.
- Run `cargo xtask bench --compare` after merge to confirm the gain on your machine.
