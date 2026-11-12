# ECAD workspace

All ECAD code lives here, as a Cargo workspace separate from CADCraft's (the parent directory).

> **Status: bounded integration experiment, not an application.**
> - **The only desktop program is a trial harness** (`apps/ecad-trial`). It is CADCraft's window
>   with the ECAD commands and one ECAD project at a time. **To build, run and try it, see
>   [`docs/quick-start.md`](docs/quick-start.md)**, the single maintained guide. It also records
>   what has been tried, and how.
> - **Projects are saved as `.ecad` files** (persistence stage 2a): Open, Save and Save As, a
>   writer lock, and recovery from interrupted or damaged files. Native Windows and Linux are
>   supported: keep projects in a local folder (`C:\dev\ecad-projects` on Windows, the Linux home
>   folder on Linux and under WSL); other file systems are unverified.
> - **A read-only debugger panel** (stage 2b) in the trial follows a selected object to its
>   ECAD IDs, model objects, transactions (with before/after values) and saved record bytes,
>   and keeps pending changes apart from saved history. The static inspector still loads
>   exported JSON snapshots (`ecad-inspect/2`).

Postponed work (the upstream sync policy of PR #5, libraries, checkpoints) is listed in
[`docs/deferred-work.md`](docs/deferred-work.md).

CADCraft crates are used by path; upstream files change only through the patches listed in
[`UPSTREAM-PATCHES.md`](UPSTREAM-PATCHES.md).

| Crate | Role | Depends on CADCraft |
| --- | --- | --- |
| `ecad-model` | authoritative document, typed IDs, ops, transactions, history | no |
| `ecad-store` | storage media: the `Medium` trait, an in-memory medium with fault injection, and (native only) files on disk, the writer lock and the steps of an atomic replacement | no |
| `ecad-file` | the `.ecad` format, version 1: records, canonical JSON, CRC and hash chain, JSON Lines, open with recovery, save with retry; on disk (native only): open with the writer lock, Save, Save As by temporary file and rename | no |
| `ecad-inspect` | read-only snapshots and the runtime event log for developers | no |
| `ecad-cadcraft` | adapter: projection, handle map, commit translation, ECAD commands, debugger snapshots and checks (provisional) | yes |
| `apps/ecad-trial` | desktop trial harness: CADCraft's window plus one ECAD project, with Open, Save and Save As, and the debugger panel | yes (UI) |
| `xtask` | `cargo xtask ci`: fmt, clippy, tests (slow set included), layering, upstream patch guard, wasm; `cargo xtask test-changed`: fast tests of the crates not pushed yet (`--with-dependents`: and the crates that depend on them, run by CI on PRs into dev); `cargo xtask wasm`: wasm build, warnings as errors; `cargo xtask clean-builds`: delete build folders and merged worktrees | — |

```sh
cd ecad
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo xtask ci                                         # all gates, slow tests included (QA gate)
cargo xtask test-changed                               # fast tests of the crates not pushed yet
cargo xtask test-changed --base origin/dev --with-dependents   # and their dependents (CI runs this on PRs into dev)
cargo xtask wasm                                       # library crates for wasm, warnings as errors (as CI)
cargo test -p ecad-cadcraft --all-features                            # the integration experiment
cargo run -p ecad-cadcraft --example experiment_snapshots -- ../target/ecad-snapshots
# then open dev/ecad-inspector/page/index.html and load the snapshots
```

Docs: [`docs/quick-start.md`](docs/quick-start.md) (run and try the app),
[`docs/experiment-report.md`](docs/experiment-report.md),
[`docs/environments.md`](docs/environments.md), and the architecture proposal in
`../docs/document-architecture-proposal.md`. Versions: product `0.1.0-dev`; incorporated
CADCraft commit in [`UPSTREAM.md`](UPSTREAM.md); format versions are independent.
