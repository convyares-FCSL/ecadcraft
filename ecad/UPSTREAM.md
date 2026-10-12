# Incorporated CADCraft

The CADCraft (upstream) commit this repository incorporates. It is tracked separately from the
ECAD product version (`0.1.0-dev`, `ecad/Cargo.toml`) and from document format versions
(`ecad_model::SCHEMA_VERSION`, `ecad_store::TEST_STORE_FORMAT`). See
`docs/document-architecture-proposal.md` §8.1.

Repository: `https://github.com/storytold/cadcraft` (branch `main`)
Commit: `00c7613f1af3c73f7157fa91d9fe2635c6f23c06` (CADCraft 0.6.2, "Release 0.6.2: rebuild with the fixed shared release workflow", 2026-10-12)

Each upstream sync PR (`upstream-sync/<date>`) updates the `Commit:` line, and
`cargo xtask patches` (run from `ecad/`) checks every file changed since this commit outside
`ecad/` against [`UPSTREAM-PATCHES.md`](UPSTREAM-PATCHES.md).
