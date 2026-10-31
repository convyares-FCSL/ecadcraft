# Brief 001: Windows CI job

Status: Draft (overseer, 2026-10-31)
Branch: `ecad/windows-ci`
Designer: no · Size: S

## Goal

Every push and pull request to `dev` builds and tests the ECAD workspace natively on Windows, so
Windows readiness is proven by CI and not by reading code. Known Windows gaps are visible, not
hidden.

## Scope

- `.github/workflows/ci.yml` (ECAD-owned, outside `ecad/`; owner approval of this brief covers it):
  - add a `windows-latest` job beside `ecad`. It runs `cargo xtask ci` from `ecad/` and also plain
    `cargo test --workspace`, without extra features;
  - install the `wasm32-unknown-unknown` target, `rustfmt` and `clippy`, and cache `ecad/`;
  - keep Linux as it is.
- Turn the `#[cfg(unix)]`-gated assertions that hide Windows gaps into explicit
  `#[cfg_attr(windows, ignore = "<gap id>: ...")]` tests, so each gap shows as ignored with its
  reason:
  - `crates/ecad-store/tests/disk_contracts.rs:76`, `:114`, `:153`;
  - `crates/ecad-file/tests/disk_contracts.rs:216`, `:309`.
- Make tests that are flaky on Windows deterministic, without changing production code:
  - review R-M3: poll for the lock after `kill`, for up to about 2 s;
  - review minor 6: release the leaked lock handle before the temp folder is dropped;
  - the missing-folder fixture `/nonexistent-folder-for-ecad-tests/...` becomes a missing folder
    inside the test's temp dir.
- Make plain `cargo test --workspace` compile. `crates/ecad-cadcraft/tests/recovery.rs:82` uses
  `faults.panic_in_commit`, which only exists with `fault-injection`. Gate that test target with
  `required-features` or `#![cfg(feature = "fault-injection")]` (test-side only).
- `cargo xtask ci` already passes locally on Windows (review §1). If it fails on the runner (shell assumptions, path separators, the patch guard's
  `git` call), fix `ecad/xtask`.

**Out of scope:** fixing the persistence gaps themselves (brief 002), path handling in the trial
(brief 003), and the root CADCraft workspace on Windows.

## Acceptance criteria

1. The Windows job runs on pull requests into `dev` and passes. Any test still failing on Windows is
   ignored with a named gap id, and the brief's report lists each one.
2. Plain `cargo test --workspace` passes on both runners.
3. No production code under `ecad/crates` or `ecad/apps` changes.
4. `ecad/docs/quick-start.md` "Automated checks" names both platforms.
5. `UPSTREAM-PATCHES.md` is unchanged in substance: `ci.yml` is already listed as ECAD-owned.

## Notes

The overseer cannot run Rust locally until the toolchain is installed (review §1). The tester checks
this brief from the CI run.
