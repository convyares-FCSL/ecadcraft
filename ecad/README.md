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

**To build, run and try the app, see [`docs/user/quick-start.md`](docs/user/quick-start.md)**; the
user guides by feature are in [`docs/user/`](docs/user/). The desktop program is `ecad-desktop` (`cargo run --release -p ecad-desktop`).

## Structure

The code is split into crates in six layers, from the project model at the bottom to the programs
at the top. See [`crates/README.md`](crates/README.md) for the layers, the rules between them
(`cargo xtask layers`) and a README for every crate.

## Commands

```sh
cd ecad
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo xtask test-changed       # fast tests of the crates you changed and have not pushed
cargo xtask layers             # the layer rules, the crate READMEs and the comment rules
cargo xtask ci                 # everything, slow tests included (the QA gate)
cargo xtask bench              # the load benchmark (release build)
cargo xtask clean-builds --root <features folder>   # delete build folders
```

How and when each check runs is in [`CLAUDE.md`](CLAUDE.md).

## Docs

Every doc is listed in [`docs/README.md`](docs/README.md). The main ones:

- [`docs/user/`](docs/user/): the user guides, starting with [`quick-start.md`](docs/user/quick-start.md).
- [`docs/developer/architecture.md`](docs/developer/architecture.md): crates, layers and the document model.
- [`docs/agents/`](docs/agents/): MCP, the control channel and the command reference.
- [`docs/reference/glossary.md`](docs/reference/glossary.md): the words used in everything a person sees.
- [`docs/design/decisions.md`](docs/design/decisions.md): the binding decisions; the source layout is
  [`docs/design/layout.md`](docs/design/layout.md).
- [`docs/roadmap.md`](docs/roadmap.md): what is planned, and the deferred work.
- [`docs/archive/`](docs/archive/README.md): the history (briefs, designs, quick starts, reviews), kept as written.
- [`UPSTREAM.md`](UPSTREAM.md) is the CADCraft commit in use; upstream files change only through the patches in [`UPSTREAM-PATCHES.md`](UPSTREAM-PATCHES.md).

Format versions are independent of the product version.
