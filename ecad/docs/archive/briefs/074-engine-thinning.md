Historical record (2026-11-17). Not current; see [developer/architecture.md](../../developer/architecture.md).

# Brief 074: engine thinning

Owner decision 2026-11-17. Runs after 072 B–G and R9 have landed, before R10 and the rename (073).

## Goal

`ecad-cadcraft` (renamed `ecad-engine` in 073) is only the adapter to CADCraft:

- translation between ECAD and CADCraft;
- commands that call the engine (resolve inputs, build the transaction, commit through `Project`);
- interaction (clicks, picking, marks, forms).

Domain rules and calculations live in `ecad-electrical`, beside 072's traits and methods. A pure
model belongs in `ecad-model`.

## What moves

The baseline counts are from `refactor/0.2.3` @ 3e54b06a.

| From (ecad-cadcraft) | Lines | What moves | To |
|---|---:|---|---|
| `cmd/` | 11,661 | Rules over `&Document`: page numbering (`default_number`, `next_number`), renumber plans, issue plans and refusals, choices, provisional items, report rows and JSON shaping | ecad-electrical (`cmd/` or the rule module) |
| `history.rs` | 2,074 | The version and branch graph model | ecad-model |
| `release.rs` | 1,470 | The pre-release check plan | ecad-electrical |
| `look.rs`, `views.rs` | 1,707 | Cable fan geometry, the traditional/fan choice, label layout | ecad-electrical |
| `props.rs` | 1,116 | Properties row building | ecad-electrical |

What stays: CADCraft calls, `Session` and `Project` glue, file writing, the clock, entity
building (`draw/`), interaction, the agent gate, and the dev tools.

## Rules

- One area per commit. Unit tests move with the code they test. End-to-end command tests stay in
  the engine.
- Behaviour and message text stay the same.
- New `xtask layers` guard: count the engine functions whose inputs are only Core types (no
  `Session`, `Project`, CADCraft types or engine handles) and that return a decision. Ratchet it
  to 0, with a reasoned allowlist only where a function truly has to stay.
- At the end, recount the crate sizes against `_tmp/crate-sizes.md` (target: engine src about
  25k lines) and report the numbers.
