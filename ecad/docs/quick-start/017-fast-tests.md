# Brief 017: fast tests

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Branch `feature/fast-tests` (brief 017)** changes how the tests are built and run, not the app:
nothing you run or see changes. Each crate's integration tests are one executable
(`tests/all/`), so the test paths below are `…/tests/all/<file>.rs`. Tests that take more than
about 2 s are marked slow: plain `cargo test` and `cargo xtask test-changed` skip them, and
`cargo xtask ci` (the QA gate, Linux and Windows CI) runs them with `--include-ignored`. To run
the whole set by hand: `cargo test --workspace --all-features -- --include-ignored`.
