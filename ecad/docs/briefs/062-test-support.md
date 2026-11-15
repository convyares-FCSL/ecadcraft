# Brief 062: Test support (code review R2)

Status: Approved (overseer, 2026-11-15, from the 0.2.3 tidy-up plan, lane R2).
Branch: `chore/test-support`, from `origin/dev` `445787b8`.
Designer: no · Size: M · Tests only, plus one cargo feature flag.
Source: `_tmp/code-review-2026-11.md`, "Tests: shared helpers and a builder" and the R2 row.
Runs in parallel with 060 (guards), 061 (crate review), 063 (UI kit) and 064 (views design).

## Goal

Make the test helpers shared, so every later refactor lane writes tests cheaply. Production
behaviour does not change.

## Scope

1. **ecad-cadcraft tests.** The copied helpers `close_all`, `place`, `reopen`, `same_as_built` and
   `tmp_dir`/`Tmp` move onto the existing `tests/all/common` (beside `H`); copies that match are
   deleted. A copy with a different signature or body is a different helper and stays.
2. **ecad-mcp.** A new `tests/all/common` module: `writer(seed)` and `assert_one_by_agent`, replacing
   the `writer()` and `one_by_agent` copies.
3. **ecad-trial.** A new `tests/all/common` module: `harness(opts)`, `Tmp` and `click`, replacing the
   `harness()` copies.
4. **`ProjectBuilder`** behind a `test-support` cargo feature of ecad-cadcraft:
   `.page().coil("-K1", at).wire(a, b).cable(..).build()`. Used in a handful of tests to prove it;
   the suite is not rewritten.

## Not in scope

No production change besides the feature flag; no new dependencies (`insta` is decided by the crate
review, lane 061); no new tests beyond those that prove the builder.

## Acceptance

- Test counts per crate (`ecad-cadcraft`, `ecad-mcp`, `ecad-trial`) stay the same or rise.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` and `cargo xtask test-changed` pass.
- Copies removed are counted in the report.
