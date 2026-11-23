# ecad-bench

Layer: Dev

## Purpose
The load benchmark. It builds synthetic projects, writes them as `.ecad` files, and times opening,
deriving, the first page and saving one change. It is a developer tool and is never shipped.

## Contents
- `generate`: builds a project of a given tier (`small`, `medium`, `large`; `PRINT_TIER` for print tests).
- `pages`, `cables`, `strips`, `ratings`, `bom`, `reports`, `wires_nets`, `symbols`, `styles`, `text`, `title_blocks`, `plc`, `connectors`, `history`, `release`, `views`, `container`: one module per area, each building or timing that area.
- `measure`, `report`: the timings, the printed table and the JSON written under `ecad/target/bench/`.
- `src/main.rs`: the command that `cargo xtask bench` runs.

## Must not depend on
Dev may use any crate, but this one has no file format, op or document code of its own: it uses the
product crates. Nothing in the product may depend on it. It sets no budgets; it fails only if a
project does not open cleanly.

## Key types
`Tier`, `TIERS`, `Generated`, `generate`.

## How to test
```bash
cargo xtask bench
```
This is a release build and is not part of `cargo xtask ci` or the pull-request checks. While
building, bench the medium tier; run the large tier once, when a change is finished. `cargo test -p
ecad-bench` runs only the small tests of the generator's shape. `translate_differential` (randomised
edits on the small project) is in the slow set.
