# ECAD workspace: agent instructions

The instructions for everything under `ecad/` are in [`CLAUDE.md`](CLAUDE.md) in this folder; they
take precedence over the repository-root `AGENTS.md` for files here.

The one rule that differs from the root file: product code is Rust only, but **developer tools
under `ecad/dev/` may be written in HTML, CSS and JavaScript** (see `docs/design/decisions.md`). Such tools ship with no product, run no server by default,
never mutate a project, and escape every value they display.

Also from `CLAUDE.md`: the user guides in [`docs/user/`](docs/user/) describe the app as it is.
Update the guide of a feature in the same commit as any change that affects what it says. Where
the docs go is listed in [`docs/README.md`](docs/README.md).
