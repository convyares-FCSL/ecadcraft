# Files changed outside `ecad/`

Every file in the upstream tree that differs from the incorporated CADCraft commit
([`UPSTREAM.md`](UPSTREAM.md)) is listed here. `cargo xtask patches` fails if a changed file is
missing. Code patches are marked in the source with `// ECAD-SEAM(owner):`.

## ECAD-owned files outside `ecad/`

These are ours. On upstream merges, conflicts are resolved by keeping ours.

| File | Why |
| --- | --- |
| `README.md` | ECAD repository front page |
| `.github/workflows/ci.yml` | CI for `dev`: CADCraft workspace and ECAD workspace |
| `docs/concept-spec.md` | ECAD concept spec (moves to `ecad/docs/` once housekeeping R11 is approved) |
| `docs/decisions.md` | ECAD decision log |
| `docs/cadcraft-assessment.md` | Phase 0 assessment |
| `docs/document-architecture-proposal.md` | architecture proposal (PR #1) |
| `docs/ecad-architecture-foundations.md` | owner's foundations note |

## Code patches to upstream crates

None yet. The generic extension interface (proposal §14, X1-X7) lands in its own PR.
