# Brief 012: load benchmark

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Branch `ecad/load-benchmark` (brief 012)**, cut from `dev` at `60c7c35`, adds the on-demand load
benchmark `cargo xtask bench` (section 8, "Measuring load time"). The app, the file format and CI
are unchanged. Evidence: the generator's shape tests (`ecad/crates/ecad-bench/tests/all/
generator_contracts.rs`), written with the branch and run by the tester and CI; one agent run of
`cargo xtask bench --tier small` on native Windows 11 Pro, 2026-11-07, which printed the table and
wrote the JSON. You have not run it yet.
