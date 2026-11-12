# ECAD features

A **feature** is a whole user-facing capability, developed on one long-lived branch
`feature/<name>` (owner decision, 2026-11-08). Briefs are steps inside a feature: they land as
commits on the feature branch, not as PRs of their own. The feature reaches `dev` by one PR when
it works; `dev` reaches `main` through the QA gate (see `CLAUDE.md`, "Branches and testing").

Up to brief 016 the work was done one PR per brief. This table maps those briefs onto features so
that new briefs know which branch they belong to. The owner corrects it when a grouping is wrong.

| Feature | Branch (from now on) | Briefs so far | State |
| --- | --- | --- | --- |
| Platform: Windows and Linux | `feature/platform` | 000, 001, 002, 003 | In `dev` |
| Upstream sync: policy, then sync to the latest CADCraft | `feature/upstream-sync` | 007, 023 | In `dev` (CADCraft 0.19.0); weekly upstream review |
| Fast tests | `feature/fast-tests` | 017 | In `dev` |
| Cables: slash, views, look, cores, references | `feature/cables` | 004, 005, 009, 010 | In `dev` |
| Usability: ECAD tools by mouse | `feature/usability` | 008 | In `dev` |
| Agent access (MCP) | `feature/agent-access` | 011 | In `dev` |
| Performance and the project file | `feature/performance` | 012, 013, 014, 015 | In `dev` |
| Connectivity and wire numbers: nets, numbering, labels, checks | `feature/connectivity` | 022 | In `dev` |
| Components, tags and starter symbols | `feature/components` | 018 (006 library file: contract only) | 018 in `dev`; `.ecadlib` after v0.1 |
| Project manager: pages, then cables, components | `feature/project-manager` | 016 | In `dev` |
| Sheets, title block, properties, revisions | `feature/sheets` | 019 | In `dev` |
| Container v2 (redb) | `feature/container-v2` | 021 | In `dev` |
| History and Branches | `feature/history`, `feature/history-merge` | 020a, 020b | In `dev` |
| Outputs: locations, PDF book, generated pages, BOM, wire and cable reports (lane A) | `feature/outputs` | 024; then 027 release | Designing |
| Device cross-references (lane B) | `feature/cross-references` | 025; then 028 terminal strips | Designing |
| Numbering: renumber, fixed, swap, provisional (lane C) | `feature/numbering` | 026; then inspector wasm, look-and-feel | Designing |

A feature branch stays open while the feature is being shaped. When a later brief extends a feature
that already reached `dev`, branch `feature/<name>` again from `dev`.

## Agreed first steps

- **Components: a general reference index (owner decision, 2026-11-08).** References are their own
  objects: a source, where the text is shown, points at a target drawn item, and the text is the
  target's page and column. Brief 014 scopes reference work per cable touched, which is enough
  while cable references are the only kind. The first step of the components feature replaces
  that with an index from target item to the references that point at it, type-agnostic, so
  component cross-references (coil and contacts, parent and child parts) need no special case.
- **Before the next feature: fast tests (owner decision, 2026-11-08).** Order: close out the open
  PRs, take `dev` into `main` through the QA gate, then restructure the tests for rapid
  development before starting any new feature. CI showed compiling and linking about 50 test
  executables (one per `tests/*.rs` file, each linking egui and wgpu, with optimisation on) took
  about 8 minutes, against 26 seconds of running. First candidate: one test executable per crate.
- **Order (owner decision, 2026-11-08):** 017 fast tests, then the upstream sync policy (PR #5's
  deferred proposal, `deferred/upstream-sync-policy.md`) and a sync to the latest CADCraft (0.18.0
  on 2026-11-06, 473 commits after our 0.15.1), then 016 project manager. QA gates fast-forward
  `main` to `dev` (no merge commits), on the owner's go-ahead each time.
