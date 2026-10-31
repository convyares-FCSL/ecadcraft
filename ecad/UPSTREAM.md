# Incorporated CADCraft

The CADCraft (upstream) commit this repository incorporates. It is tracked separately from the
ECAD product version (`0.1.0-dev`, `ecad/Cargo.toml`) and from document format versions
(`ecad_model::SCHEMA_VERSION`, `ecad_store::TEST_STORE_FORMAT`). See
`docs/document-architecture-proposal.md` §8.1.

Repository: `https://github.com/storytold/cadcraft` (release tag `v0.15.1`)
Commit: `660e15c9f494ce647e87de10adbb01a8786e0901` (CADCraft 0.15.1, "Release 0.15.1", 2026-10-30)

Each upstream sync PR (`upstream-sync/<date>`) updates the `Commit:` line, and
`cargo xtask patches` (run from `ecad/`) checks every file changed since this commit outside
`ecad/` against [`UPSTREAM-PATCHES.md`](UPSTREAM-PATCHES.md).
