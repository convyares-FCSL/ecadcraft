# ECAD workspace: instructions for agents

This folder holds all ECAD code as a Cargo workspace of its own. CADCraft (the parent directory's
`crates/`, `apps/`, `xtask/`) is the upstream drawing platform and is used by path dependency.

- Read `../docs/document-architecture-proposal.md` (principles in Part A are fixed; Part B is
  provisional) and `../docs/decisions.md` first.
- Branch from `dev`; every change arrives as a PR into `dev`. Never merge your own PR: green CI is
  not approval.
- Before every commit run `cargo xtask ci` from `ecad/` (fmt, clippy `-D warnings`, tests, layers,
  upstream patch guard, wasm). Build with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0` to keep
  the target directory near 6 GB.
- `ecad-model` and `ecad-store` must never depend on a `cadcraft-*` crate. Only `ecad-cadcraft`
  touches the engine.
- Changing any file outside `ecad/` is an upstream patch: list it in `UPSTREAM-PATCHES.md` and mark
  code with `// ECAD-SEAM(owner):`. Keep ECAD semantics out of upstream crates.
- Never-crash rules apply: no `unwrap`/`expect`/`panic!` in non-test code, no `unsafe`, errors
  through each crate's error type.
- Style: `rustfmt` at 120 columns; crate `//!` docs say what the crate excludes; public items
  document `# Semantics`, `# Errors`, `# Example` where useful; contract tests are table-driven in
  `tests/<area>_contracts.rs`.
- Versions: product `0.1.0-dev`, release tags `ecad-v…`, CADCraft commit in `UPSTREAM.md`, format
  versions independent of the product version.
