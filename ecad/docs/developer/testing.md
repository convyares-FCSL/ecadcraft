# Testing and benches

ECAD is Rust: what builds and passes clippy mostly works. So tests are cheap enough to run on every commit, they
run only for what changed, and the full suite runs once, at the QA gate (the pull request from `dev` into
`main`). This page covers where tests go, the fast and slow sets, how to run only what changed, what CI runs,
and the load benchmark.

## Where tests go

**One integration test executable per crate.** Each crate has `tests/all/main.rs`, and every test file is a
module of it, under `tests/all/<feature>/` (the feature folder of the code it covers), one module per file.
Shared helpers are in `tests/all/common/`. Tests are named `<module path>::<test>`, so
`cargo test -p <crate> <name>` runs one module's tests. Never add a new `tests/*.rs` file: each one is its own
executable and links egui and wgpu again, which is where most of the build time goes.

Two crates keep a second executable on purpose: `ecad-store/tests/disk_contracts.rs`, and
`ecad-engine/tests/sheet_switch.rs`, which switches a process-wide setting and so needs a process of its own.

Which crate a test belongs to:

| Test | Crate and folder |
| --- | --- |
| A rule: a `&Document` in, a result out | `ecad-electrical/tests/all/<feature>/` |
| A session, drawing or click test, or a command run through the engine | `ecad-engine/tests/all/<feature>/` |
| A screen, panel or dialog | `ecad-app/tests/all/<feature>/` |
| An agent's view of a command (MCP contract tests) | `ecad/apps/ecad-mcp/tests/all/` |
| The file format | `ecad-file/tests/all/` (with the golden file in `tests/golden/`) |
| The xtask guards | unit tests in `ecad/dev/xtask/src/` |

- **Contract tests are table-driven**: a table of cases, one loop that checks them, in a module named
  `<area>_contracts`.
- **Test modules say what they test**, never which brief or role they came from.
- **Small projects** for engine tests come from `ecad_engine::dev::test_support::ProjectBuilder` (the
  `test-support` feature, on only for tests), which builds them with the same commands a person runs.
- **Verify mode is on in tests.** Every crate turns on `ecad-model`'s `verify-invariants` through its
  dev-dependencies, so after every transaction the full invariant check runs and must agree with the scoped
  one. App builds, debug or release, take the fast path; `ECAD_VERIFY_INVARIANTS=1` turns it on at run time.
- **UI tests** run the app headless (`egui_kittest`). A test that writes screenshots writes them only when
  `$ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists.

## The fast set and the slow set

A test that takes more than about 2 s is marked `#[ignore = "slow: <why>"]`. Everyday runs (`cargo test`,
`cargo xtask test-changed`) skip it. The slow set runs only at the QA gate (`cargo xtask ci`, or CI's full
suite), or when the owner asks for it after large changes. Workers and testers never run `--include-ignored`.

A test too slow at `opt-level = 0` is marked slow, not optimised: ECAD's own crates build unoptimised for dev and
test so a change recompiles quickly ([`build.md`](build.md#profiles)).

**Child-process helpers.** A test that needs a fresh process starts its own test binary as a child. The helper
it runs is named `child_*`, carries its own `#[ignore = "run as a child process"]` reason (or similar), and does
nothing unless the test that starts it set `ECAD_TEST_CHILD`. So an `--include-ignored` run is always safe: run
directly, a helper returns at once. A new helper follows the same pattern.

## Running only what changed

```sh
cargo xtask test-changed                    # fast tests of the crates changed since your last push
cargo xtask test-changed --dry-run          # only name the crates
cargo xtask test-changed -- cable_view      # arguments after -- go to the test executables (a filter)
cargo xtask test-changed --base origin/dev  # changes since another ref
cargo xtask test-changed --with-dependents  # also every crate that depends on them (what CI runs)
```

- The default base is the branch's upstream, so it tests what was not pushed yet; a branch never pushed is
  compared with `origin/dev`. Committed, staged, unstaged and untracked files all count.
- A changed file selects the innermost crate whose folder holds it. Markdown files and files in no crate (docs,
  CI) select nothing; `ecad/examples/` selects `ecad-mcp`, which embeds the demo's commands. The workspace
  manifest and lock file, the repository root manifest, cargo configuration and the toolchain file select every
  crate. A change to a CADCraft crate used by path selects the ECAD crates that use it.
- It runs `cargo test --all-features` for those crates, never the slow set.

The everyday checks:

```sh
cargo fmt --all -- --check          # before each commit
cargo xtask test-changed            # before each commit
cargo clippy --workspace --all-targets --all-features -- -D warnings   # before each push (compiles every test)
cargo xtask patches                 # only when files outside ecad/ changed
```

The crates that depend on yours are not tested locally: CI runs their fast tests on the pull request. Never run
the whole workspace's tests by hand. Documentation-only changes need no cargo at all. When a change touches
files outside `ecad/`, also build and clippy CADCraft's root workspace and run the affected CADCraft crates'
tests.

## What CI runs

**Pull requests into `dev` (the quick check, Linux):** `cargo fmt --all -- --check`, clippy with all targets and
features (so every test must still compile), then `cargo xtask test-changed --base origin/dev
--with-dependents`. A push to `dev` builds every test binary for the cache the pull requests restore, and runs
nothing.

**The QA gate** (pull requests into `main`, pushes to `main`, and by hand): CADCraft's own workspace, and the ECAD
workspace on Linux and natively on Windows. For ECAD, CI runs what `cargo xtask ci` runs, split into parallel
jobs:

- `cargo xtask lint`: fmt, clippy with all features and with the default ones, `layers` and `patches`;
- every test with all features, the slow set included, through cargo-nextest in three shards: `ecad-bench`'s
  tests except its slow translation test, and the rest (with that test) split in two;
- the fast tests with default features, and the doctests (nextest does not run doctests);
- on Windows: clippy with both feature sets, every fast test with both, the doctests, and the slow tests except
  those that only compute in memory. Those are named in the `ci-windows` profile of `ecad/.config/nextest.toml`
  and run on Linux; a new slow test runs on Windows until it is added there;
- the large-tier load bench (`cargo xtask bench --tier large`), kept as an artifact.

The nextest profiles (`ecad/.config/nextest.toml`) report a test slower than 60 s, retry nothing (a flaky test
fails the gate), report every failure and write a JUnit file. Locally, `cargo test` and `cargo xtask
test-changed` are enough; nextest is optional.

`cargo xtask ci` runs the same locally: `lint`, then `cargo test --workspace --all-features --
--include-ignored`. The fault-injection tests run only with all features, never in normal builds.

## The load benchmark

`cargo xtask bench` builds synthetic projects, writes them as `.ecad` files and times opening, the first page,
an edit, saving, drawing edits, page switching, list rebuilds and more per area. It is a release build, run on
demand: never part of `cargo xtask ci`, `cargo test --workspace` or the pull-request checks. The large tier runs
at the QA gate.

```sh
cargo xtask bench                         # small, medium and the print project
cargo xtask bench --tier small            # a couple of minutes
cargo xtask bench --tier medium
cargo xtask bench --tier large --runs 1   # long; once, when a change is finished
cargo xtask bench --tier small --keep <folder>                     # keep the projects to open in the app
cargo xtask bench --tier small --compare target/bench/<earlier>.json
```

| Tier | Pages | Devices | Wires | Cables |
| --- | ---: | ---: | ---: | ---: |
| small | 10 | 100 | 500 | 50 |
| medium | 100 | 1,000 | 5,000 | 500 |
| large | 500 | 5,000 | 25,000 | 2,500 |
| print | 25 | 250 | 1,250 | 125 |

- The same tier always gives the same project (a deterministic generator).
- Each figure is the median of 5 runs (`--runs N` to change). `--tier` takes a comma-separated list, or `all`.
- The table is printed and written as JSON under `ecad/target/bench/`, with the machine, OS, commit and build;
  an existing file is never overwritten. `--compare` prints the change per row against an earlier file.
- `--keep <folder>` keeps the generated projects so you can open them in the app; it refuses to replace a
  project open in another session.
- It sets no pass or fail limits; it fails only if a project does not open cleanly. Results depend on the
  machine, so compare runs from the same machine.

**When to bench.** While building, bench on the **medium** project. Run the large tier once, at the end, and only
for the operations the change sets targets for. Print Book and release use the **print** project (25 pages),
never the 100- or 500-page tiers. A view that keeps an update map needs large-tier evidence that a full rebuild
costs more than about 5 ms per edit ([`architecture.md`](architecture.md#derived-views)).

`cargo test -p ecad-bench` runs only the small tests of the generator's shape; `translate_differential`
(randomised edits on the small project) is in the slow set.

## Start-up timing

Set `ECAD_STARTUP_TIMING=1` before starting `ecad-desktop` to see where the time goes before the first frame:
see [`build.md`](build.md#start-up-timing).
