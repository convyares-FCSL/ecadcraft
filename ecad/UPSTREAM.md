# Incorporated CADCraft

The CADCraft (upstream) commit this repository incorporates. It is tracked separately from the
ECAD product version (`0.1.0-dev`, `ecad/Cargo.toml`) and from document format versions
(`ecad_model::SCHEMA_VERSION`, and the container and record versions in `ecad-file`, such as
`ecad_file::CONTAINER_VERSION`). See
`docs/document-architecture-proposal.md` §8.1.

Repository: `https://github.com/storytold/cadcraft` (release tag `v0.19.0`)
Commit: `227e3c385b20129cf525775e3d9cbe78b722b3bb` (CADCraft 0.19.0, "Release 0.19.0", 2026-11-09)

Each upstream sync PR (`upstream-sync/<date>`) updates the `Commit:` line, and
`cargo xtask patches` (run from `ecad/`) checks every file changed since this commit outside
`ecad/` against [`UPSTREAM-PATCHES.md`](UPSTREAM-PATCHES.md).
