# Incorporated CADCraft

The CADCraft (upstream) commit this repository incorporates. It is tracked separately from the
ECAD product version (`0.3.0-dev`, `ecad/Cargo.toml`) and from the document format versions in
`ecad-file` (the record version `ecad_file::SCHEMA_VERSION` and the container version
`ecad_file::schema::CONTAINER_VERSION`). See
`ecad/docs/developer/architecture.md`.

Repository: `https://github.com/storytold/cadcraft` (branch `main`, untagged)
Commit: `414e54428222627ad173b595d17954788183c684` (CADCraft main on the 0.21.0 line, "Merge pull request #2760", 2026-11-26)

Each upstream sync PR (`upstream-sync/<date>`) updates the `Commit:` line, and
`cargo xtask patches` (run from `ecad/`) checks every file changed since this commit outside
`ecad/` against [`UPSTREAM-PATCHES.md`](UPSTREAM-PATCHES.md). That file also names the upstream files
ECAD compiles unchanged by `#[path]` (upstream's control server), which the sync checks too.
