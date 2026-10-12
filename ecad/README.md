# ECAD workspace

All ECAD code lives here, as a Cargo workspace separate from CADCraft's (the parent directory).
CADCraft crates are used by path; upstream files change only through the patches listed in
[`UPSTREAM-PATCHES.md`](UPSTREAM-PATCHES.md).

| Crate | Role | Depends on CADCraft |
| --- | --- | --- |
| `ecad-model` | authoritative document, typed IDs, ops, transactions, history | no |
| `ecad-store` | provisional **test-only** store (not the `.ecad` format) | no |
| `ecad-inspect` | read-only snapshots and the runtime event log for developers | no |
| `ecad-cadcraft` | adapter: projection, handle map, commit translation, ECAD commands (provisional) | yes |
| `xtask` | `cargo xtask ci`: fmt, clippy, tests, layering, upstream patch guard, wasm | — |

```sh
cd ecad
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cargo xtask ci                                         # all gates
cargo test -p ecad-cadcraft --all-features                            # the integration experiment
cargo run -p ecad-cadcraft --example experiment_snapshots -- ../target/ecad-snapshots
# then open tools/inspector/index.html and load the snapshots
```

Docs: [`docs/experiment-report.md`](docs/experiment-report.md),
[`docs/environments.md`](docs/environments.md), and the architecture proposal in
`../docs/document-architecture-proposal.md`. Versions: product `0.1.0-dev`; incorporated
CADCraft commit in [`UPSTREAM.md`](UPSTREAM.md); format versions are independent.
