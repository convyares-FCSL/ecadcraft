# ECAD

ECAD is an electrical CAD application built on CADCraft's drawing platform. It is **pre-alpha** and
**desktop-only** (Windows and Linux). The last release is `ecad-v0.2.0` (14 November 2026); this
branch carries the next one, `0.3.0-dev`. All ECAD code lives here, as a Cargo workspace separate
from CADCraft's (the parent directory), which it uses by path.

What it does today (the plan, release by release, is in [`docs/roadmap.md`](docs/roadmap.md)):

- **Projects as `.ecad` files**, with Open, Save and Save As, a writer lock, recovery from damaged
  files, an undo history that is saved with the project, and branches of that history.
- **Devices, wires and cables:** tagged components with several symbols each, wire categories and
  numbers, cables and their cores, connectors with mating halves.
- **Terminal strips and PLC:** strip pages built from the drawing; PLC racks, cards, channels and an I/O list.
- **Checks and reports:** rule checks, wire and cable ratings, a bill of materials, and the PDF book.
- **Agents over MCP:** an agent can read a project and, if allowed, draw in the running app. Its
  edits are recorded under its own name.

## Run it

**To build, run and try the app, see [`docs/quick-start.md`](docs/quick-start.md)**, the single
maintained guide. The desktop program is currently `ecad-trial` (being renamed `ecad-desktop`).

## Structure

The code is split into crates in six layers, from the project model at the bottom to the programs
at the top. See [`crates/README.md`](crates/README.md) for the layers, the rules between them
(`cargo xtask layers`) and a README for every crate.

## Commands

```sh
cd ecad
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo xtask test-changed       # fast tests of the crates you changed and have not pushed
cargo xtask layers             # the layer rules and the crate READMEs
cargo xtask ci                 # everything, slow tests included (the QA gate)
cargo xtask bench              # the load benchmark (release build)
cargo xtask clean-builds --root C:\dev\ecad-features   # delete build folders
```

How and when each check runs is in [`CLAUDE.md`](CLAUDE.md).

## Docs

- [`docs/quick-start.md`](docs/quick-start.md): run and try the app.
- [`docs/roadmap.md`](docs/roadmap.md) and [`docs/features.md`](docs/features.md): what is planned and which branch holds each feature.
- [`docs/glossary.md`](docs/glossary.md): the words used in everything a person sees.
- [`docs/deferred-work.md`](docs/deferred-work.md): postponed work.
- [`../docs/document-architecture-proposal.md`](../docs/document-architecture-proposal.md): the architecture.
- [`docs/experiment-report.md`](docs/experiment-report.md): the report of the first integration experiment (October 2026), kept as history.
- [`UPSTREAM.md`](UPSTREAM.md) is the CADCraft commit in use; upstream files change only through the patches in [`UPSTREAM-PATCHES.md`](UPSTREAM-PATCHES.md).

Format versions are independent of the product version.
