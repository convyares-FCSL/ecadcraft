# ecad-model

Layer: Core

## Purpose
The ECAD project itself: objects with stable ids, the ops that change them, transactions, and the
history that undo and redo extend. It is the first crate in Core, so everything else builds on it.
It has no drawing engine, no UI, no file encoding and no I/O.

## Contents
- `doc`: the objects (`Document`, `Page`, `Component`, `Cable`, `Style`…) and their reverse indexes.
- `op`: ops, each one self-inverting. `ledger`: `Transaction`s applied with rollback, plus linear undo and redo.
- `invariant`: the rules a document must keep, checked in full or only for what a transaction touched.
- `connectivity`, `wiring`, `derive`: nets (kept up to date by every op), wire numbers, and results worked out from the document.
- `merge`, `numbering`, `diff`: merging two documents, renumbering by rule, comparing two states.
- `book`, `sheet`, `title_block`, `style`, `tag`, `links`, `geom`, `id`: pages and sheets, style catalogues, tags, typed ids, points.
- `look`: the kinds of text (`TextKind`), a kind's resolved style (`TextStyle`), and `TextLook`, through which the electrical rules ask how a text is drawn and how wide it is.
- `registry`: the defaults registry's mechanism: entries, kinds, levels, resolution, the built-in data sets each crate declares (`registry::data`) and style catalogue fields and built-ins (`registry::styles`).
- `palette`: the canvas palette: `Color`, the paper and dark sheet colours, markers, wire colours and dashes, line weights in mm, the sheet font's bytes.
- `dev`: the verify-mode switch (a developer and test hook).

## Must not depend on
No other ECAD crate and no `cadcraft-*` crate (principle P10), so an upstream CADCraft change cannot
change the document or replay. No UI. No OS access: it reads no environment, clock or file, and the
program passes in anything it needs (for example `dev::set_verify_invariants`).

## Key types
`Document`, `Op`, `Transaction`, `Ledger`, `Author`, `Connectivity`, `ModelError` (all re-exported
from the crate root).

## How to test
```bash
cargo test -p ecad-model
```
Tests are in `tests/all/` (one executable). Test runs turn on the `verify-invariants` feature,
which checks the full invariants after every transaction. Slow tests are marked
`#[ignore = "slow: …"]` and run only at the QA gate.
