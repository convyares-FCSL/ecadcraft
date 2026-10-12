# Decision log

One line per decision, newest last: date, decision, reason. Fixed decisions from the concept spec
(`docs/concept-spec.md`, "Kickoff prompt") are not repeated here unless they change.

| Date | Decision | Reason |
| --- | --- | --- |
| 2026-10-12 | `dev` is the integration branch; every major feature is built on a branch cut from `dev` and merged back when its tests pass | Keeps `dev` always green and each feature reviewable on its own |
| 2026-10-12 | The concept spec lives in `docs/concept-spec.md` and is the reference for product scope | One place for the product definition, versioned with the code |
