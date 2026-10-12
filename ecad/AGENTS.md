# ECAD workspace: agent instructions

The instructions for everything under `ecad/` are in [`CLAUDE.md`](CLAUDE.md) in this folder; they
take precedence over the repository-root `AGENTS.md` for files here.

The one rule that differs from the root file: product code is Rust only, but **developer tools
under `ecad/tools/` may be written in HTML, CSS and JavaScript** (owner decision, 2026-10-12,
recorded in `../docs/decisions.md`). Such tools ship with no product, run no server by default,
never mutate a project, and escape every value they display.
