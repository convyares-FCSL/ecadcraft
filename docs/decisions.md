# Decision log

One line per decision, newest last: date, decision, reason. Fixed decisions from the concept spec
(`docs/concept-spec.md`, "Kickoff prompt") are not repeated here unless they change.

| Date | Decision | Reason |
| --- | --- | --- |
| 2026-10-12 | `dev` is the integration branch; every major feature is built on a branch cut from `dev` and merged back when its tests pass | Keeps `dev` always green and each feature reviewable on its own |
| 2026-10-12 | The concept spec lives in `docs/concept-spec.md` and is the reference for product scope | One place for the product definition, versioned with the code |
| 2026-10-12 | Soft fork: CADCraft crates stay as they are; ECAD code lives in new `ecad-*` crates; engine edits are limited to three marked seams (extension commands, commit observer, domain state slot) | Keeps upstream engine improvements mergeable; see `docs/cadcraft-assessment.md` (proposed, awaiting owner approval) |
| 2026-10-12 | Ledger IDs are allocated by the ledger, never reused from CADCraft handles; the bridge keeps a ledger-id ⇄ handle map | Handles are per-drawing database keys that change on import, paste and explode |
| 2026-10-12 | `ecad-ledger`, `ecad-topology` and `ecad-infer` do not depend on `cadcraft-engine` | Domain rules stay testable without a `Session` and independent of UI and engine churn |
| 2026-10-12 | Open: pages as one projected `Drawing` each, or one paper-space layout each | Phase 1 needs one page; decide in Phase 2 with the multi-page work |
| 2026-10-12 | CI (`.github/workflows/ci.yml`) runs fmt, clippy `-D warnings` and the workspace tests on pushes and pull requests to `dev` | Feature branches merge back only when green |
| 2026-10-12 | Document architecture proposal revision 2 approved as the basis for the bounded integration experiment; Part B integration choices stay provisional until demonstrated | Owner review of PR #1 |
| 2026-10-12 | Versioning: ECAD development version `0.1.0-dev`; release tags `ecad-v…`; the incorporated CADCraft commit is tracked separately (`ecad/UPSTREAM.md`); document format versions are independent of product versions | Owner decision; keeps product, upstream and file-format changes separately visible |
| 2026-10-12 | Focused-review outcomes: the generic engine extension interface is retained provisionally and not offered upstream yet; page documents carry the project path as adapter metadata; losing the selection after recovery is acceptable (IDs and content must survive); unavailable pages stay labelled read-only placeholders with no extra engine hook; owner locking is enforced through the API; placement rotation is stored in radians and converted at command/UI boundaries; drafting-command restrictions stay until parameterised tests pass | Owner review of PRs #1-#4 |
| 2026-10-12 | Developer tools under `ecad/tools/` may use HTML/CSS/JavaScript (the inspector stays a static page); product code stays Rust only | Owner decision on the inspector review thread; recorded in `ecad/CLAUDE.md` and `ecad/AGENTS.md` |
| 2026-10-12 | The upstream cleanup direction (PR #5) is approved with the earlier refinements; implementation is deferred until after the desktop trial, and PR #5 stays the policy proposal | Owner decision; one active workstream at a time |
| 2026-10-12 | A minimal ECAD desktop trial harness (`ecad/apps/ecad-trial`) is approved as bounded experiment work: no project persistence (Save and Save As report that persistence is unavailable), closing an edited project offers Cancel or Discard, optional diagnostic snapshot export | Owner approval for a manual WSL trial; not broader feature work |
